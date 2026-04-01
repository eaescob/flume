use mlua::{Lua, Result as LuaResult, Value};

/// Apply sandbox restrictions to the Lua VM.
/// Removes dangerous globals that could allow file system access,
/// process execution, or arbitrary code loading.
pub fn apply_sandbox(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();

    // Remove dangerous top-level functions
    globals.set("dofile", Value::Nil)?;
    globals.set("loadfile", Value::Nil)?;

    // Restrict os module — keep os.time, os.date, os.clock, os.difftime
    // Remove os.execute, os.remove, os.rename, os.tmpname, os.getenv, os.exit
    if let Ok(os_table) = globals.get::<mlua::Table>("os") {
        os_table.set("execute", Value::Nil)?;
        os_table.set("remove", Value::Nil)?;
        os_table.set("rename", Value::Nil)?;
        os_table.set("tmpname", Value::Nil)?;
        os_table.set("getenv", Value::Nil)?;
        os_table.set("exit", Value::Nil)?;
    }

    // Restrict io module — remove io.popen, io.open (scripts use flume.config instead)
    if let Ok(io_table) = globals.get::<mlua::Table>("io") {
        io_table.set("popen", Value::Nil)?;
        io_table.set("open", Value::Nil)?;
        io_table.set("input", Value::Nil)?;
        io_table.set("output", Value::Nil)?;
        io_table.set("tmpfile", Value::Nil)?;
    }

    // Remove debug library (can be used to escape sandbox)
    globals.set("debug", Value::Nil)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_blocks_os_execute() {
        let lua = Lua::new();
        apply_sandbox(&lua).unwrap();

        let result = lua.load("os.execute('echo hi')").exec();
        assert!(result.is_err());
    }

    #[test]
    fn sandbox_blocks_io_popen() {
        let lua = Lua::new();
        apply_sandbox(&lua).unwrap();

        let result = lua.load("io.popen('ls')").exec();
        assert!(result.is_err());
    }

    #[test]
    fn sandbox_blocks_io_open() {
        let lua = Lua::new();
        apply_sandbox(&lua).unwrap();

        let result = lua.load("io.open('/etc/passwd')").exec();
        assert!(result.is_err());
    }

    #[test]
    fn sandbox_blocks_dofile() {
        let lua = Lua::new();
        apply_sandbox(&lua).unwrap();

        let result = lua.load("dofile('/etc/passwd')").exec();
        assert!(result.is_err());
    }

    #[test]
    fn sandbox_blocks_loadfile() {
        let lua = Lua::new();
        apply_sandbox(&lua).unwrap();

        let result = lua.load("loadfile('/etc/passwd')").exec();
        assert!(result.is_err());
    }

    #[test]
    fn sandbox_allows_os_time() {
        let lua = Lua::new();
        apply_sandbox(&lua).unwrap();

        let result = lua.load("return os.time()").eval::<i64>();
        assert!(result.is_ok());
        assert!(result.unwrap() > 0);
    }

    #[test]
    fn sandbox_allows_string_and_table() {
        let lua = Lua::new();
        apply_sandbox(&lua).unwrap();

        let result = lua
            .load(r#"return string.format("hello %s", "world")"#)
            .eval::<String>();
        assert_eq!(result.unwrap(), "hello world");
    }

    #[test]
    fn sandbox_blocks_debug() {
        let lua = Lua::new();
        apply_sandbox(&lua).unwrap();

        let result = lua.load("debug.getinfo(1)").exec();
        assert!(result.is_err());
    }
}
