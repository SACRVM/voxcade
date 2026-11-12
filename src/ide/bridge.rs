//! The MCP bridge: the running IDE within reach of `voxcade mcp`, and so of
//! the agent in its console, on this machine only. The IDE listens on
//! 127.0.0.1 at a free port and writes the port and a random token into
//! `config_dir()/ide/<game>.lock` (readable by the user alone, gone when the
//! IDE closes); `voxcade mcp` started in the game folder finds it there and
//! passes the IDE's tools on (`mcp.rs`). A call is one JSON line,
//! `{"token", "tool", "args"}`, answered by one, `{"content", "isError"}`;
//! one without the token is refused. The IDE's loop answers the calls
//! (`calls`), woken by the thread that takes them.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{Value, json};

/// How long a call waits for the IDE's answer, and `voxcade mcp` for the
/// IDE.
const PATIENCE: Duration = Duration::from_secs(30);

/// Where the IDE on the game in `game_dir` leaves its lock.
pub fn lock_path(game_dir: &Path) -> PathBuf {
    crate::settings::config_dir().join("ide").join(format!("{}.lock", crate::settings::game_name(game_dir)))
}

/// A call to the IDE: the tool and its arguments, and where its answer
/// goes.
pub struct Call {
    pub tool: String,
    pub args: Value,
    reply: mpsc::Sender<Value>,
}

impl Call {
    /// Answers with `content` (MCP's content: text, images).
    pub fn answer(self, content: Vec<Value>) {
        let _ = self.reply.send(json!({ "content": content, "isError": false }));
    }

    /// Answers that it could not be done, and why.
    pub fn fail(self, why: &str) {
        let _ = self.reply.send(json!({ "content": [{ "type": "text", "text": why }], "isError": true }));
    }
}

/// The IDE's end of the bridge.
pub struct Bridge {
    lock: PathBuf,
    calls: mpsc::Receiver<Call>,
    /// Its port, and the word to its thread to stop listening (another
    /// game opened in the IDE's window).
    port: u16,
    stop: Arc<AtomicBool>,
}

impl Bridge {
    /// Listens for the IDE on the game in `game_dir`, its lock at `lock`;
    /// `wake` wakes the IDE's loop (from another thread) when a call came.
    pub fn open(lock: &Path, game_dir: &Path, wake: Arc<dyn Fn() + Send + Sync>) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").context("listening for voxcade mcp")?;
        let port = listener.local_addr()?.port();
        let token = token();
        let game = game_dir.canonicalize().unwrap_or_else(|_| game_dir.to_path_buf());
        write_private(lock, &json!({ "port": port, "token": token, "game": game, "pid": std::process::id() }).to_string())?;
        let (send, calls) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        std::thread::Builder::new().name("mcp bridge".into()).spawn(move || {
            for stream in listener.incoming().flatten() {
                if stopped.load(Ordering::Acquire) {
                    break;
                }
                let (send, token, wake) = (send.clone(), token.clone(), wake.clone());
                std::thread::spawn(move || {
                    let _ = serve(stream, &token, &send, &*wake);
                });
            }
        })?;
        Ok(Self { lock: lock.to_path_buf(), calls, port, stop })
    }

    /// The calls that came since last asked.
    pub fn calls(&self) -> Vec<Call> {
        self.calls.try_iter().collect()
    }
}

impl Drop for Bridge {
    /// The lock goes, and the thread stops listening (a call of our own
    /// wakes it to hear so), its port free again.
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.lock);
        self.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

/// Takes one call on `stream`: refused without `token`, else handed to
/// the IDE (woken) and its answer written back.
fn serve(stream: TcpStream, token: &str, calls: &mpsc::Sender<Call>, wake: &(dyn Fn() + Send + Sync)) -> Result<()> {
    stream.set_read_timeout(Some(PATIENCE))?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    let msg: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
    let answer = if msg.get("token").and_then(Value::as_str) != Some(token) {
        json!({ "content": [{ "type": "text", "text": "the IDE's token is missing or wrong" }], "isError": true })
    } else {
        let (reply, answer) = mpsc::channel();
        let tool = msg.get("tool").and_then(Value::as_str).unwrap_or("").to_string();
        calls.send(Call { tool, args: msg.get("args").cloned().unwrap_or(json!({})), reply })?;
        wake();
        answer.recv_timeout(PATIENCE).unwrap_or_else(|_| json!({ "content": [{ "type": "text", "text": "the IDE did not answer" }], "isError": true }))
    };
    let mut stream = stream;
    writeln!(stream, "{answer}")?;
    Ok(())
}

/// Calls `tool` of the IDE whose lock is at `lock`: its content, or what
/// went wrong (no IDE running for the game, among others).
pub fn call(lock: &Path, tool: &str, args: &Value) -> std::result::Result<Vec<Value>, String> {
    let text = std::fs::read_to_string(lock).map_err(|_| "the IDE is not running on this game (voxcade ide <game> opens it)".to_string())?;
    let lock: Value = serde_json::from_str(&text).map_err(|e| format!("the IDE's lock: {e}"))?;
    let port = lock["port"].as_u64().ok_or("the IDE's lock has no port")?;
    let mut stream = TcpStream::connect(("127.0.0.1", port as u16)).map_err(|_| "the IDE is not running on this game any more (voxcade ide <game> opens it)".to_string())?;
    stream.set_read_timeout(Some(PATIENCE)).map_err(|e| e.to_string())?;
    writeln!(stream, "{}", json!({ "token": lock["token"], "tool": tool, "args": args })).map_err(|e| format!("calling the IDE: {e}"))?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line).map_err(|e| format!("the IDE's answer: {e}"))?;
    let answer: Value = serde_json::from_str(&line).map_err(|e| format!("the IDE's answer: {e}"))?;
    let content = answer["content"].as_array().cloned().unwrap_or_default();
    if answer["isError"].as_bool() == Some(true) {
        Err(content.iter().filter_map(|c| c["text"].as_str()).collect::<Vec<_>>().join("\n"))
    } else {
        Ok(content)
    }
}

/// A random token, 128 bits as hex.
fn token() -> String {
    random().iter().map(|b| format!("{b:02x}")).collect()
}

/// 128 random bits: from the system's randomness where it has a file for
/// it, else from the standard library's (seeded from it).
pub(crate) fn random() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    let read = std::fs::File::open("/dev/urandom").and_then(|mut f| std::io::Read::read_exact(&mut f, &mut bytes));
    if read.is_err() {
        use std::hash::{BuildHasher, Hasher};
        for half in bytes.chunks_mut(8) {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
            half.copy_from_slice(&h.finish().to_le_bytes());
        }
    }
    bytes
}

/// Writes `text` to `path` readable by the user alone (on Unix; Windows
/// keeps a user's config folder to them).
fn write_private(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path).with_context(|| format!("writing {}", path.display()))?;
    file.write_all(text.as_bytes())?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A bridge answering its calls in the background, as the IDE's loop
    /// does: `view_state` with a line of text.
    pub(crate) fn running(name: &str) -> (Bridge, PathBuf, crate::scratch::Scratch) {
        let scratch = crate::scratch::Scratch::new(&format!("bridge-{name}"));
        let lock = scratch.join("ide.lock");
        let bridge = Bridge::open(&lock, Path::new("games/voxkeep"), Arc::new(|| {})).unwrap();
        (bridge, lock, scratch)
    }

    pub(crate) fn answer_calls(bridge: &Bridge) {
        for call in bridge.calls() {
            if call.tool == "view_state" {
                call.answer(vec![json!({ "type": "text", "text": "main.lua open" })]);
            } else {
                let why = format!("no tool {}", call.tool);
                call.fail(&why);
            }
        }
    }

    /// A call with the token in the lock is answered by the IDE; one
    /// without it, or with another, is refused before the IDE hears of it.
    #[test]
    fn the_bridge_needs_the_token() {
        let (bridge, lock, _scratch) = running("token");
        let port = serde_json::from_str::<Value>(&std::fs::read_to_string(&lock).unwrap()).unwrap()["port"].as_u64().unwrap();
        #[cfg(unix)]
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&lock).unwrap().permissions()) & 0o777, 0o600, "the user's alone");
        for wrong in [json!({ "tool": "view_state" }), json!({ "token": "guess", "tool": "view_state" })] {
            let mut stream = TcpStream::connect(("127.0.0.1", port as u16)).unwrap();
            writeln!(stream, "{wrong}").unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            assert!(line.contains("token is missing or wrong"), "{line}");
        }
        assert!(bridge.calls().is_empty(), "the IDE heard nothing");
        let caller = std::thread::spawn({
            let lock = lock.clone();
            move || call(&lock, "view_state", &json!({}))
        });
        while !caller.is_finished() {
            answer_calls(&bridge);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(caller.join().unwrap().unwrap()[0]["text"], "main.lua open");
        let port = bridge.port;
        drop(bridge);
        assert!(!lock.exists(), "the lock goes with the IDE");
        // The thread stopped listening: nobody answers on its port (a while
        // for the thread to hear it).
        let until = std::time::Instant::now() + Duration::from_secs(3);
        while TcpStream::connect(("127.0.0.1", port)).is_ok() {
            assert!(std::time::Instant::now() < until, "still listening");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(call(&lock, "view_state", &json!({})).unwrap_err().contains("not running"));
    }
}
