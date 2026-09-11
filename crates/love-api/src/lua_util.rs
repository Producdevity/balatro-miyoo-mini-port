// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use mlua::prelude::*;

/// Parse LÖVE color from either (r,g,b,a) args or ({r,g,b,a}) table arg
pub fn parse_color(args: &LuaMultiValue) -> [f32; 4] {
    let mut iter = args.iter();
    match iter.next() {
        Some(LuaValue::Table(t)) => [
            t.get::<f32>(1).unwrap_or(0.0),
            t.get::<f32>(2).unwrap_or(0.0),
            t.get::<f32>(3).unwrap_or(0.0),
            t.get::<f32>(4).unwrap_or(1.0),
        ],
        first => {
            let r = lua_val_to_f32(first);
            let g = lua_val_to_f32(iter.next());
            let b = lua_val_to_f32(iter.next());
            let a = lua_val_to_f32_or(iter.next(), 1.0);
            [r, g, b, a]
        }
    }
}

fn lua_val_to_f32(v: Option<&LuaValue>) -> f32 {
    match v {
        Some(LuaValue::Number(n)) => *n as f32,
        Some(LuaValue::Integer(n)) => *n as f32,
        _ => 0.0,
    }
}

fn lua_val_to_f32_or(v: Option<&LuaValue>, default: f32) -> f32 {
    match v {
        Some(LuaValue::Number(n)) => *n as f32,
        Some(LuaValue::Integer(n)) => *n as f32,
        Some(LuaValue::Nil) | None => default,
        _ => default,
    }
}

/// Parse LÖVE color from args starting at the given offset
pub fn parse_color_offset(args: &LuaMultiValue, offset: usize) -> [f32; 4] {
    let r = lua_val_to_f32(args.get(offset));
    let g = lua_val_to_f32(args.get(offset + 1));
    let b = lua_val_to_f32(args.get(offset + 2));
    let a = lua_val_to_f32_or(args.get(offset + 3), 1.0);
    [r, g, b, a]
}

pub fn color_f32_to_u8(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0] * 255.0).clamp(0.0, 255.0) as u8,
        (c[1] * 255.0).clamp(0.0, 255.0) as u8,
        (c[2] * 255.0).clamp(0.0, 255.0) as u8,
        (c[3] * 255.0).clamp(0.0, 255.0) as u8,
    ]
}

/// Register multiple no-op functions on a table
pub fn register_noop_fns(lua: &Lua, table: &LuaTable, names: &[&str]) -> LuaResult<()> {
    for name in names {
        table.set(*name, lua.create_function(|_, _: LuaMultiValue| Ok(()))?)?;
    }
    Ok(())
}
