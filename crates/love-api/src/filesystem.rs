// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use mlua::prelude::*;
use std::sync::Arc;

use crate::state::SharedState;

pub fn register(lua: &Lua, love: &LuaTable, state: Arc<SharedState>) -> LuaResult<()> {
    let fs = lua.create_table()?;

    // love.filesystem.read(container_or_name, name_or_nil)
    // Supports both read(name) and read("string", name)
    {
        let s = Arc::clone(&state);
        fs.set(
            "read",
            lua.create_function(move |lua, args: LuaMultiValue| {
                let path = match args.len() {
                    0 => return Ok((LuaNil, 0i64)),
                    1 => match args.get(0) {
                        Some(LuaValue::String(s)) => s.to_string_lossy().to_string(),
                        _ => return Ok((LuaNil, 0i64)),
                    },
                    _ => {
                        // read("string", filename) or read(filename, size)
                        match args.get(1) {
                            Some(LuaValue::String(s)) => s.to_string_lossy().to_string(),
                            _ => match args.get(0) {
                                Some(LuaValue::String(s)) => s.to_string_lossy().to_string(),
                                _ => return Ok((LuaNil, 0i64)),
                            },
                        }
                    }
                };

                let source = s.game_source.lock();
                match source.read_file(&path) {
                    Ok(bytes) => {
                        let len = bytes.len() as i64;
                        // First try reading from save dir
                        Ok((
                            LuaValue::String(
                                lua.create_string(&bytes)
                                    .map_err(|e| LuaError::external(e))?,
                            ),
                            len,
                        ))
                    }
                    Err(_) => {
                        // Try save directory
                        drop(source);
                        let save_path = s.save_dir.join(&path);
                        match std::fs::read(&save_path) {
                            Ok(bytes) => {
                                let len = bytes.len() as i64;
                                Ok((
                                    LuaValue::String(
                                        lua.create_string(&bytes)
                                            .map_err(|e| LuaError::external(e))?,
                                    ),
                                    len,
                                ))
                            }
                            Err(_) => Ok((LuaNil, 0i64)),
                        }
                    }
                }
            })?,
        )?;
    }

    // love.filesystem.getInfo(path [, filtertype]) -> info_table or nil
    {
        let s = Arc::clone(&state);
        fs.set(
            "getInfo",
            lua.create_function(move |lua, args: LuaMultiValue| {
                let path = match args.get(0) {
                    Some(LuaValue::String(s)) => s.to_string_lossy().to_string(),
                    _ => return Ok(LuaNil),
                };

                let source = s.game_source.lock();

                // Check game source first
                if source.file_exists(&path) {
                    let t = lua.create_table()?;
                    t.set("type", "file")?;
                    t.set("size", 0i64)?;
                    t.set("modtime", 0i64)?;
                    return Ok(LuaValue::Table(t));
                }

                if source.is_directory(&path) {
                    let t = lua.create_table()?;
                    t.set("type", "directory")?;
                    t.set("size", 0i64)?;
                    t.set("modtime", 0i64)?;
                    return Ok(LuaValue::Table(t));
                }

                drop(source);

                // Check save directory
                let save_path = s.save_dir.join(&path);
                if save_path.exists() {
                    let t = lua.create_table()?;
                    if save_path.is_dir() {
                        t.set("type", "directory")?;
                    } else {
                        t.set("type", "file")?;
                        t.set(
                            "size",
                            std::fs::metadata(&save_path)
                                .map(|m| m.len() as i64)
                                .unwrap_or(0),
                        )?;
                    }
                    t.set("modtime", 0i64)?;
                    return Ok(LuaValue::Table(t));
                }

                Ok(LuaNil)
            })?,
        )?;
    }

    // love.filesystem.getDirectoryItems(dir)
    {
        let s = Arc::clone(&state);
        fs.set(
            "getDirectoryItems",
            lua.create_function(move |lua, path: String| {
                // Merge items from game source and save directory
                let source = s.game_source.lock();
                let mut items = source.list_directory(&path);
                drop(source);

                // Also list from save directory
                let save_path = s.save_dir.join(&path);
                if save_path.is_dir() {
                    if let Ok(entries) = std::fs::read_dir(&save_path) {
                        for entry in entries.filter_map(|e| e.ok()) {
                            let name = entry.file_name().to_string_lossy().into_owned();
                            if !items.contains(&name) {
                                items.push(name);
                            }
                        }
                    }
                }

                let t = lua.create_sequence_from(items)?;
                Ok(t)
            })?,
        )?;
    }

    // love.filesystem.write(name, data [, size])
    {
        let s = Arc::clone(&state);
        fs.set(
            "write",
            lua.create_function(move |_, (path, data): (String, mlua::String)| {
                let full = s.save_dir.join(&path);
                if let Some(parent) = full.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                let name = full
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("save");
                let temporary = full.with_file_name(format!(".{name}.tmp"));
                {
                    use std::io::Write;
                    let mut file = std::fs::File::create(&temporary).map_err(LuaError::external)?;
                    file.write_all(&data.as_bytes())
                        .map_err(LuaError::external)?;
                    file.sync_data().map_err(LuaError::external)?;
                }
                std::fs::rename(&temporary, &full).map_err(LuaError::external)?;
                Ok(true)
            })?,
        )?;
    }

    // love.filesystem.append(name, data [, size])
    {
        let s = Arc::clone(&state);
        fs.set(
            "append",
            lua.create_function(move |_, (path, data): (String, mlua::String)| {
                use std::io::Write;
                let full = s.save_dir.join(&path);
                if let Some(parent) = full.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&full)
                    .map_err(LuaError::external)?;
                file.write_all(&data.as_bytes())
                    .map_err(LuaError::external)?;
                Ok(true)
            })?,
        )?;
    }

    // love.filesystem.remove(name)
    {
        let s = Arc::clone(&state);
        fs.set(
            "remove",
            lua.create_function(move |_, path: String| {
                let full = s.save_dir.join(&path);
                if full.is_dir() {
                    std::fs::remove_dir_all(&full).ok();
                } else {
                    std::fs::remove_file(&full).ok();
                }
                Ok(true)
            })?,
        )?;
    }

    // love.filesystem.createDirectory(name)
    {
        let s = Arc::clone(&state);
        fs.set(
            "createDirectory",
            lua.create_function(move |_, path: String| {
                let full = s.save_dir.join(&path);
                std::fs::create_dir_all(&full).ok();
                Ok(true)
            })?,
        )?;
    }

    // love.filesystem.getSourceBaseDirectory()
    {
        let s = Arc::clone(&state);
        fs.set(
            "getSourceBaseDirectory",
            lua.create_function(move |_, ()| {
                let source = s.game_source.lock();
                Ok(source.source_base_directory())
            })?,
        )?;
    }

    // love.filesystem.getSaveDirectory()
    {
        let s = Arc::clone(&state);
        fs.set(
            "getSaveDirectory",
            lua.create_function(move |_, ()| Ok(s.save_dir.to_string_lossy().into_owned()))?,
        )?;
    }

    // love.filesystem.getIdentity()
    fs.set("getIdentity", lua.create_function(|_, ()| Ok("Balatro"))?)?;

    // love.filesystem.setIdentity(name)
    fs.set(
        "setIdentity",
        lua.create_function(|_, _name: String| Ok(()))?,
    )?;

    love.set("filesystem", fs)?;
    Ok(())
}

/// Set up the custom require loader for game files
pub fn setup_require(lua: &Lua, state: Arc<SharedState>) -> LuaResult<()> {
    let package: LuaTable = lua.globals().get("package")?;

    // Use loaders (LuaJIT/5.1) or searchers (5.2+)
    let loaders: LuaTable = package
        .get::<LuaTable>("loaders")
        .or_else(|_| package.get::<LuaTable>("searchers"))?;

    let loader = lua.create_function(move |lua, modname: String| {
        // Convert module name to file path
        let path = if modname.ends_with(".lua") {
            modname.clone()
        } else {
            modname.replace('.', "/") + ".lua"
        };

        let source = state.game_source.lock();
        match source.read_file(&path) {
            Ok(bytes) => {
                drop(source);
                let code = String::from_utf8_lossy(&bytes);
                let chunk = lua
                    .load(code.as_ref())
                    .set_name(format!("@{}", path))
                    .into_function()?;
                Ok(LuaMultiValue::from_vec(vec![LuaValue::Function(chunk)]))
            }
            Err(_) => {
                // Also try with forward slash variant
                let alt_path = modname.replace('.', "/") + ".lua";
                match source.read_file(&alt_path) {
                    Ok(bytes) => {
                        drop(source);
                        let code = String::from_utf8_lossy(&bytes);
                        let chunk = lua
                            .load(code.as_ref())
                            .set_name(format!("@{}", alt_path))
                            .into_function()?;
                        Ok(LuaMultiValue::from_vec(vec![LuaValue::Function(chunk)]))
                    }
                    Err(_) => {
                        let msg = format!("\n\tno file '{}' in game archive", path);
                        Ok(LuaMultiValue::from_vec(vec![LuaValue::String(
                            lua.create_string(&msg)?,
                        )]))
                    }
                }
            }
        }
    })?;

    // Insert our loader at position 2 (after preload)
    let len = loaders.raw_len();
    for i in (2..=len).rev() {
        let v: LuaValue = loaders.raw_get(i)?;
        loaders.raw_set(i + 1, v)?;
    }
    loaders.raw_set(2, loader)?;

    Ok(())
}

/// Set up preload stubs for modules that need special handling
pub fn setup_preloads(lua: &Lua) -> LuaResult<()> {
    let package: LuaTable = lua.globals().get("package")?;
    let preload: LuaTable = package.get("preload")?;

    // luasteam stub — init returns false so Balatro skips Steam
    preload.set(
        "luasteam",
        lua.create_function(|lua, ()| {
            let t = lua.create_table()?;
            t.set("init", lua.create_function(|_, _self: LuaValue| Ok(false))?)?;
            t.set(
                "shutdown",
                lua.create_function(|_, _self: LuaValue| Ok(()))?,
            )?;
            Ok(t)
        })?,
    )?;

    // https stub for crash reporter
    preload.set(
        "https",
        lua.create_function(|lua, ()| {
            let t = lua.create_table()?;
            t.set(
                "request",
                lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
            )?;
            Ok(t)
        })?,
    )?;

    // bit library (LuaJIT-compatible) for lua51
    preload.set(
        "bit",
        lua.create_function(|lua, ()| {
            let bit = lua.create_table()?;

            bit.set("tobit", lua.create_function(|_, x: f64| Ok(x as i32))?)?;
            bit.set(
                "tohex",
                lua.create_function(|_, (x, n): (i32, Option<i32>)| {
                    let n = n.unwrap_or(8);
                    if n < 0 {
                        Ok(format!("{:0>width$X}", x as u32, width = (-n) as usize))
                    } else {
                        Ok(format!("{:0>width$x}", x as u32, width = n as usize))
                    }
                })?,
            )?;
            bit.set("bnot", lua.create_function(|_, x: i32| Ok(!x))?)?;
            bit.set(
                "band",
                lua.create_function(|_, args: LuaMultiValue| {
                    let mut result: i32 = -1; // all bits set
                    for arg in args.iter() {
                        if let Some(n) = arg.as_integer() {
                            result &= n as i32;
                        } else if let Some(n) = arg.as_number() {
                            result &= n as i32;
                        }
                    }
                    Ok(result)
                })?,
            )?;
            bit.set(
                "bor",
                lua.create_function(|_, args: LuaMultiValue| {
                    let mut result: i32 = 0;
                    for arg in args.iter() {
                        if let Some(n) = arg.as_integer() {
                            result |= n as i32;
                        } else if let Some(n) = arg.as_number() {
                            result |= n as i32;
                        }
                    }
                    Ok(result)
                })?,
            )?;
            bit.set(
                "bxor",
                lua.create_function(|_, args: LuaMultiValue| {
                    let mut result: i32 = 0;
                    for arg in args.iter() {
                        if let Some(n) = arg.as_integer() {
                            result ^= n as i32;
                        } else if let Some(n) = arg.as_number() {
                            result ^= n as i32;
                        }
                    }
                    Ok(result)
                })?,
            )?;
            bit.set(
                "lshift",
                lua.create_function(|_, (x, n): (i32, u32)| Ok(x.wrapping_shl(n & 31)))?,
            )?;
            bit.set(
                "rshift",
                lua.create_function(|_, (x, n): (i32, u32)| {
                    Ok((x as u32).wrapping_shr(n & 31) as i32)
                })?,
            )?;
            bit.set(
                "arshift",
                lua.create_function(|_, (x, n): (i32, u32)| Ok(x.wrapping_shr(n & 31)))?,
            )?;
            bit.set(
                "rol",
                lua.create_function(|_, (x, n): (i32, u32)| {
                    Ok((x as u32).rotate_left(n & 31) as i32)
                })?,
            )?;
            bit.set(
                "ror",
                lua.create_function(|_, (x, n): (i32, u32)| {
                    Ok((x as u32).rotate_right(n & 31) as i32)
                })?,
            )?;
            bit.set(
                "bswap",
                lua.create_function(|_, x: i32| Ok((x as u32).swap_bytes() as i32))?,
            )?;

            Ok(bit)
        })?,
    )?;

    Ok(())
}
