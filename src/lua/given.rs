//! What a script hands the engine, read as the script sees it. A table the
//! script may only read (a state's tables in `on_draw`, a global after the
//! start, `thing.props`, a top-level state) is a view: an empty table whose
//! metatable reads through to the table it shows (the prelude's
//! `readonly`, `frozen` and `engine_state`, each kept in its `shows`). The
//! engine reads tables raw (`pairs`, `raw_get`, `raw_len`), which finds a
//! view empty; `Given` (a table) and `GivenValue` (any value, a table
//! among them) are the table a view shows instead, so a call reads what
//! the script would read. A reader takes its arguments so, and the values
//! in them (`pairs::<Value, GivenValue>`, `raw_get::<Option<Given>>`).
//!
//! Only for reading: a table met so is never written, kept or handed back
//! to a script (that would give it the table behind its view). A top-level
//! state handed whole (`player.state`) is read as its plain part: the
//! engine's keys (`score`, `hp`) are not in it.

use mlua::{FromLua, Lua, Table, Value};

/// The prelude's `shows` (each view to the table it shows, weak keys), in
/// the Lua state's app data.
pub(super) struct Shows(pub Table);

/// The table `t` is to the script: the one it shows if it is a view.
pub fn seen(lua: &Lua, t: Table) -> mlua::Result<Table> {
    // A table without a metatable (every literal) is no view: no lookup.
    if t.metatable().is_none() {
        return Ok(t);
    }
    let Some(shows) = lua.app_data_ref::<Shows>() else { return Ok(t) };
    Ok(shows.0.raw_get::<Option<Table>>(&t)?.unwrap_or(t))
}

/// A table a script hands the engine, as the script sees it.
pub struct Given(pub Table);

impl FromLua for Given {
    fn from_lua(v: Value, lua: &Lua) -> mlua::Result<Self> {
        match v {
            Value::Table(t) => Ok(Given(seen(lua, t)?)),
            other => Err(mlua::Error::FromLuaConversionError { from: other.type_name(), to: "table".into(), message: None }),
        }
    }
}

impl std::ops::Deref for Given {
    type Target = Table;

    fn deref(&self) -> &Table {
        &self.0
    }
}

/// A value a script hands the engine, a table as the script sees it.
pub struct GivenValue(pub Value);

impl FromLua for GivenValue {
    fn from_lua(v: Value, lua: &Lua) -> mlua::Result<Self> {
        Ok(GivenValue(match v {
            Value::Table(t) => Value::Table(seen(lua, t)?),
            v => v,
        }))
    }
}
