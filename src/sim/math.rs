//! The simulation's own sine, cosine, arctangent and cube root. The standard
//! library's come from the operating system's maths library, and those
//! differ in the last bit between Linux, Windows and macOS: a replay
//! recorded on one would run apart on another. These (the `libm` crate, a
//! port of musl's) give the same bits everywhere. Square roots are exact
//! everywhere and need nothing. Test: `the_simulation_does_its_own_math`.

pub fn sin(x: f32) -> f32 {
    libm::sinf(x)
}

pub fn cos(x: f32) -> f32 {
    libm::cosf(x)
}

/// Sine and cosine at once.
pub fn sin_cos(x: f32) -> (f32, f32) {
    libm::sincosf(x)
}

pub fn atan2(y: f32, x: f32) -> f32 {
    libm::atan2f(y, x)
}

pub fn cbrt(x: f32) -> f32 {
    libm::cbrtf(x)
}

#[cfg(test)]
mod tests {
    /// Nothing in the simulation calls the system's maths library: every
    /// sine, cosine and the like goes through this module. The engine's core,
    /// the kits' rules and the Lua door (not how the shooter draws,
    /// `draw.rs`, nor the tests).
    #[test]
    fn the_simulation_does_its_own_math() {
        let banned = [
            ".sin(", ".cos(", ".sin_cos(", ".tan(", ".asin(", ".acos(", ".atan(", ".atan2(", ".sinh(", ".cosh(", ".tanh(",
            ".exp(", ".exp2(", ".ln(", ".log(", ".log2(", ".log10(", ".powf(", ".powi(", ".cbrt(", ".hypot(",
            "from_angle(", "angle_between(", "Quat::",
        ];
        let mut found = Vec::new();
        let files = ["src/sim", "src/shooter", "src/platformer", "src/lua"].into_iter().flat_map(|dir| std::fs::read_dir(dir).expect("reading the simulation").map(|e| e.unwrap().path()));
        for path in files {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if ["math.rs", "draw.rs", "tests.rs"].contains(&name.as_str()) || path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for (n, line) in text.lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                for b in banned.iter().filter(|b| code.contains(*b)) {
                    found.push(format!("{}:{}: {b}", path.display(), n + 1));
                }
            }
        }
        assert!(found.is_empty(), "the system's maths in the simulation (use sim::math):\n{}", found.join("\n"));
    }
}
