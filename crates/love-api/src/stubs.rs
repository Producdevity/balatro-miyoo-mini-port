// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use mlua::prelude::*;

use crate::state::SharedState;
use std::sync::Arc;

// --- Base64 encode/decode ---

const B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_CHARS[((triple >> 18) & 0x3F) as usize]);
        out.push(B64_CHARS[((triple >> 12) & 0x3F) as usize]);
        if chunk.len() > 1 {
            out.push(B64_CHARS[((triple >> 6) & 0x3F) as usize]);
        } else {
            out.push(b'=');
        }
        if chunk.len() > 2 {
            out.push(B64_CHARS[(triple & 0x3F) as usize]);
        } else {
            out.push(b'=');
        }
    }
    out
}

fn base64_decode(data: &[u8]) -> Result<Vec<u8>, &'static str> {
    fn b64_val(c: u8) -> Result<u32, &'static str> {
        match c {
            b'A'..=b'Z' => Ok((c - b'A') as u32),
            b'a'..=b'z' => Ok((c - b'a' + 26) as u32),
            b'0'..=b'9' => Ok((c - b'0' + 52) as u32),
            b'+' => Ok(62),
            b'/' => Ok(63),
            b'=' => Ok(0),
            _ => Err("invalid base64 character"),
        }
    }
    // Filter whitespace
    let clean: Vec<u8> = data
        .iter()
        .copied()
        .filter(|&c| c != b'\n' && c != b'\r' && c != b' ')
        .collect();
    if clean.len() % 4 != 0 {
        return Err("invalid base64 length");
    }
    let mut out = Vec::with_capacity(clean.len() / 4 * 3);
    for chunk in clean.chunks(4) {
        let a = b64_val(chunk[0])?;
        let b = b64_val(chunk[1])?;
        let c = b64_val(chunk[2])?;
        let d = b64_val(chunk[3])?;
        let triple = (a << 18) | (b << 12) | (c << 6) | d;
        out.push((triple >> 16) as u8);
        if chunk[2] != b'=' {
            out.push((triple >> 8) as u8);
        }
        if chunk[3] != b'=' {
            out.push(triple as u8);
        }
    }
    Ok(out)
}

// --- Hex encode/decode ---

fn hex_encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 2);
    for &b in data {
        out.push(b"0123456789abcdef"[(b >> 4) as usize]);
        out.push(b"0123456789abcdef"[(b & 0xF) as usize]);
    }
    out
}

fn hex_decode(data: &[u8]) -> Result<Vec<u8>, &'static str> {
    fn hex_val(c: u8) -> Result<u8, &'static str> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            _ => Err("invalid hex character"),
        }
    }
    if data.len() % 2 != 0 {
        return Err("invalid hex length");
    }
    let mut out = Vec::with_capacity(data.len() / 2);
    for chunk in data.chunks(2) {
        out.push((hex_val(chunk[0])? << 4) | hex_val(chunk[1])?);
    }
    Ok(out)
}

/// Register compatibility APIs that do not need a native backend.
pub fn register(lua: &Lua, love: &LuaTable, state: Arc<SharedState>) -> LuaResult<()> {
    register_thread(lua, love)?;
    register_mouse(lua, love, Arc::clone(&state))?;
    register_joystick(lua, love)?;
    register_touch(lua, love)?;
    register_data(lua, love)?;
    register_arg(lua, love)?;
    register_math(lua, love)?;
    register_image(lua, love, state)?;
    Ok(())
}

fn register_thread(lua: &Lua, love: &LuaTable) -> LuaResult<()> {
    let thread = lua.create_table()?;

    thread.set(
        "newThread",
        lua.create_function(|lua, _code: LuaValue| {
            let t = lua.create_table()?;
            t.set(
                "start",
                lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
            )?;
            t.set(
                "isRunning",
                lua.create_function(|_, _self: LuaValue| Ok(false))?,
            )?;
            t.set(
                "getError",
                lua.create_function(|_, _self: LuaValue| Ok(LuaNil))?,
            )?;
            t.set("wait", lua.create_function(|_, _self: LuaValue| Ok(()))?)?;
            Ok(LuaValue::Table(t))
        })?,
    )?;

    thread.set(
        "getChannel",
        lua.create_function(|lua, _name: String| {
            let ch = lua.create_table()?;
            ch.set(
                "push",
                lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
            )?;
            ch.set("pop", lua.create_function(|_, _self: LuaValue| Ok(LuaNil))?)?;
            ch.set(
                "demand",
                lua.create_function(|_, _args: LuaMultiValue| Ok(LuaNil))?,
            )?;
            ch.set(
                "supply",
                lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
            )?;
            ch.set(
                "peek",
                lua.create_function(|_, _self: LuaValue| Ok(LuaNil))?,
            )?;
            ch.set(
                "getCount",
                lua.create_function(|_, _self: LuaValue| Ok(0i32))?,
            )?;
            ch.set("clear", lua.create_function(|_, _self: LuaValue| Ok(()))?)?;
            Ok(LuaValue::Table(ch))
        })?,
    )?;

    love.set("thread", thread)?;
    Ok(())
}

fn register_mouse(lua: &Lua, love: &LuaTable, state: Arc<SharedState>) -> LuaResult<()> {
    let mouse = lua.create_table()?;

    {
        let s = Arc::clone(&state);
        mouse.set(
            "getPosition",
            lua.create_function(move |_, ()| {
                let x = *s.mouse_x.lock() as f64;
                let y = *s.mouse_y.lock() as f64;
                Ok((x, y))
            })?,
        )?;
    }
    {
        let s = Arc::clone(&state);
        mouse.set(
            "getX",
            lua.create_function(move |_, ()| Ok(*s.mouse_x.lock() as f64))?,
        )?;
    }
    {
        let s = Arc::clone(&state);
        mouse.set(
            "getY",
            lua.create_function(move |_, ()| Ok(*s.mouse_y.lock() as f64))?,
        )?;
    }
    mouse.set("setVisible", lua.create_function(|_, _b: bool| Ok(()))?)?;
    mouse.set("isVisible", lua.create_function(|_, ()| Ok(false))?)?;
    mouse.set("setGrabbed", lua.create_function(|_, _b: bool| Ok(()))?)?;
    mouse.set("isGrabbed", lua.create_function(|_, ()| Ok(false))?)?;
    mouse.set(
        "setRelativeMode",
        lua.create_function(|_, _b: bool| Ok(()))?,
    )?;
    mouse.set("getRelativeMode", lua.create_function(|_, ()| Ok(false))?)?;
    {
        let s = Arc::clone(&state);
        mouse.set(
            "isDown",
            lua.create_function(move |_, args: LuaMultiValue| {
                let buttons = s.mouse_buttons_down.read();
                for arg in args.iter() {
                    let btn = match arg {
                        LuaValue::Integer(n) => *n as u8,
                        LuaValue::Number(n) => *n as u8,
                        _ => continue,
                    };
                    if buttons.contains(&btn) {
                        return Ok(true);
                    }
                }
                Ok(false)
            })?,
        )?;
    }

    mouse.set(
        "setCursor",
        lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
    )?;
    mouse.set(
        "getSystemCursor",
        lua.create_function(|lua, _name: String| {
            let cursor = lua.create_table()?;
            cursor.set(
                "type",
                lua.create_function(|_, _self: LuaValue| Ok("Cursor"))?,
            )?;
            Ok(LuaValue::Table(cursor))
        })?,
    )?;
    mouse.set(
        "newCursor",
        lua.create_function(|lua, _args: LuaMultiValue| {
            let cursor = lua.create_table()?;
            cursor.set(
                "type",
                lua.create_function(|_, _self: LuaValue| Ok("Cursor"))?,
            )?;
            Ok(LuaValue::Table(cursor))
        })?,
    )?;

    love.set("mouse", mouse)?;
    Ok(())
}

fn register_joystick(lua: &Lua, love: &LuaTable) -> LuaResult<()> {
    let joystick = lua.create_table()?;

    joystick.set(
        "getJoysticks",
        lua.create_function(|lua, ()| Ok(lua.create_table()?))?,
    )?;
    joystick.set(
        "loadGamepadMappings",
        lua.create_function(|_, _s: LuaValue| Ok(()))?,
    )?;
    joystick.set("getJoystickCount", lua.create_function(|_, ()| Ok(0i32))?)?;

    love.set("joystick", joystick)?;
    Ok(())
}

fn register_touch(lua: &Lua, love: &LuaTable) -> LuaResult<()> {
    let touch = lua.create_table()?;

    touch.set(
        "getTouches",
        lua.create_function(|lua, ()| Ok(lua.create_table()?))?,
    )?;

    love.set("touch", touch)?;
    Ok(())
}

fn register_data(lua: &Lua, love: &LuaTable) -> LuaResult<()> {
    let data = lua.create_table()?;

    // love.data.compress(container, format, rawstring [, level])
    data.set(
        "compress",
        lua.create_function(|lua, args: LuaMultiValue| {
            use flate2::write::DeflateEncoder;
            use flate2::Compression;
            use std::io::Write;

            // Parse args: compress("string", "deflate", data [, level])
            // or compress("data", "deflate", data [, level])
            let mut iter = args.iter();
            let _container = iter.next(); // "string" or "data"
            let _format = iter.next(); // "deflate", "lz4", "zlib", "gzip"
            let raw_data = match iter.next() {
                Some(LuaValue::String(s)) => s.as_bytes().to_vec(),
                _ => return Err(LuaError::external("compress: expected string data")),
            };
            let level = match iter.next() {
                Some(LuaValue::Integer(n)) => *n as u32,
                Some(LuaValue::Number(n)) => *n as u32,
                _ => 6,
            };

            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::new(level.min(9)));
            encoder.write_all(&raw_data).map_err(LuaError::external)?;
            let compressed = encoder.finish().map_err(LuaError::external)?;
            lua.create_string(&compressed)
        })?,
    )?;

    // love.data.decompress(container, format, compressed_data)
    data.set(
        "decompress",
        lua.create_function(|lua, args: LuaMultiValue| {
            use flate2::read::DeflateDecoder;
            use std::io::Read;

            let mut iter = args.iter();
            let _container = iter.next();
            let _format = iter.next();
            let compressed = match iter.next() {
                Some(LuaValue::String(s)) => s.as_bytes().to_vec(),
                _ => return Err(LuaError::external("decompress: expected string data")),
            };

            let mut decoder = DeflateDecoder::new(&compressed[..]);
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(LuaError::external)?;
            lua.create_string(&decompressed)
        })?,
    )?;

    // love.data.encode(container, format, sourcestring)
    data.set(
        "encode",
        lua.create_function(|lua, args: LuaMultiValue| {
            let mut iter = args.iter();
            let _container = iter.next();
            let format = match iter.next() {
                Some(LuaValue::String(s)) => s.to_string_lossy().to_string(),
                _ => "base64".to_string(),
            };
            let source = match iter.next() {
                Some(LuaValue::String(s)) => s.as_bytes().to_vec(),
                _ => return Err(LuaError::external("encode: expected string")),
            };
            match format.as_str() {
                "base64" => lua.create_string(&base64_encode(&source)),
                "hex" => lua.create_string(&hex_encode(&source)),
                _ => lua.create_string(&source),
            }
        })?,
    )?;

    // love.data.decode(container, format, sourcestring)
    data.set(
        "decode",
        lua.create_function(|lua, args: LuaMultiValue| {
            let mut iter = args.iter();
            let _container = iter.next();
            let format = match iter.next() {
                Some(LuaValue::String(s)) => s.to_string_lossy().to_string(),
                _ => "base64".to_string(),
            };
            let source = match iter.next() {
                Some(LuaValue::String(s)) => s.as_bytes().to_vec(),
                _ => return Err(LuaError::external("decode: expected string")),
            };
            match format.as_str() {
                "base64" => {
                    let decoded = base64_decode(&source)
                        .map_err(|e| LuaError::external(format!("base64 decode error: {}", e)))?;
                    lua.create_string(&decoded)
                }
                "hex" => {
                    let decoded = hex_decode(&source)
                        .map_err(|e| LuaError::external(format!("hex decode error: {}", e)))?;
                    lua.create_string(&decoded)
                }
                _ => lua.create_string(&source),
            }
        })?,
    )?;

    love.set("data", data)?;
    Ok(())
}

fn register_arg(lua: &Lua, love: &LuaTable) -> LuaResult<()> {
    let arg = lua.create_table()?;

    arg.set(
        "parseGameArguments",
        lua.create_function(|lua, _args: LuaValue| Ok(lua.create_table()?))?,
    )?;

    love.set("arg", arg)?;
    Ok(())
}

fn register_math(lua: &Lua, love: &LuaTable) -> LuaResult<()> {
    let math_table = lua.create_table()?;

    // love.math.random — delegate to Lua's math.random
    math_table.set(
        "random",
        lua.create_function(|lua, args: LuaMultiValue| {
            let math: LuaTable = lua.globals().get("math")?;
            let random: LuaFunction = math.get("random")?;
            random.call::<LuaMultiValue>(args)
        })?,
    )?;

    // love.math.setRandomSeed
    math_table.set(
        "setRandomSeed",
        lua.create_function(|lua, seed: LuaValue| {
            let math: LuaTable = lua.globals().get("math")?;
            let randomseed: LuaFunction = math.get("randomseed")?;
            randomseed.call::<()>(seed)?;
            Ok(())
        })?,
    )?;

    // love.math.getRandomSeed
    math_table.set(
        "getRandomSeed",
        lua.create_function(|_, ()| Ok((0i64, 0i64)))?,
    )?;

    // love.math.noise(x [, y, z, w])
    math_table.set(
        "noise",
        lua.create_function(|_, _args: LuaMultiValue| Ok(0.5f64))?,
    )?;

    // love.math.newRandomGenerator
    math_table.set(
        "newRandomGenerator",
        lua.create_function(|lua, _args: LuaMultiValue| {
            let rng = lua.create_table()?;
            rng.set(
                "random",
                lua.create_function(|lua, args: LuaMultiValue| {
                    let math: LuaTable = lua.globals().get("math")?;
                    let random: LuaFunction = math.get("random")?;
                    random.call::<LuaMultiValue>(args)
                })?,
            )?;
            rng.set(
                "setSeed",
                lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
            )?;
            rng.set(
                "getSeed",
                lua.create_function(|_, _self: LuaValue| Ok((0i64, 0i64)))?,
            )?;
            Ok(LuaValue::Table(rng))
        })?,
    )?;

    // love.math.newTransform([x, y, angle, sx, sy, ox, oy, kx, ky])
    math_table.set(
        "newTransform",
        lua.create_function(|lua, args: LuaMultiValue| {
            let x = match args.get(0) {
                Some(LuaValue::Number(n)) => *n as f32,
                _ => 0.0,
            };
            let y = match args.get(1) {
                Some(LuaValue::Number(n)) => *n as f32,
                _ => 0.0,
            };
            let angle = match args.get(2) {
                Some(LuaValue::Number(n)) => *n as f32,
                _ => 0.0,
            };
            let sx = match args.get(3) {
                Some(LuaValue::Number(n)) => *n as f32,
                _ => 1.0,
            };
            let sy = match args.get(4) {
                Some(LuaValue::Number(n)) => *n as f32,
                _ => sx,
            };
            let ox = match args.get(5) {
                Some(LuaValue::Number(n)) => *n as f32,
                _ => 0.0,
            };
            let oy = match args.get(6) {
                Some(LuaValue::Number(n)) => *n as f32,
                _ => 0.0,
            };

            // Build affine matrix: translate(x,y) * rotate(angle) * scale(sx,sy) * translate(-ox,-oy)
            let cos = angle.cos();
            let sin = angle.sin();
            let a = sx * cos;
            let b = sy * -sin;
            let c = sx * sin;
            let d = sy * cos;
            let tx = x + ox * (-a) + oy * (-b);
            let ty = y + ox * (-c) + oy * (-d);

            let t = lua.create_table()?;
            t.set("a", a)?;
            t.set("b", b)?;
            t.set("c", c)?;
            t.set("d", d)?;
            t.set("tx", tx)?;
            t.set("ty", ty)?;

            // Methods
            let translate_fn =
                lua.create_function(|_, (self_tbl, dx, dy): (LuaTable, f32, f32)| {
                    let a: f32 = self_tbl.get("a")?;
                    let b: f32 = self_tbl.get("b")?;
                    let c: f32 = self_tbl.get("c")?;
                    let d: f32 = self_tbl.get("d")?;
                    let old_tx: f32 = self_tbl.get("tx")?;
                    let old_ty: f32 = self_tbl.get("ty")?;
                    self_tbl.set("tx", old_tx + a * dx + b * dy)?;
                    self_tbl.set("ty", old_ty + c * dx + d * dy)?;
                    Ok(LuaValue::Table(self_tbl))
                })?;
            t.set("translate", translate_fn)?;

            let scale_fn =
                lua.create_function(|_, (self_tbl, sx, sy): (LuaTable, f32, Option<f32>)| {
                    let sy = sy.unwrap_or(sx);
                    let a: f32 = self_tbl.get("a")?;
                    let b: f32 = self_tbl.get("b")?;
                    let c: f32 = self_tbl.get("c")?;
                    let d: f32 = self_tbl.get("d")?;
                    self_tbl.set("a", a * sx)?;
                    self_tbl.set("b", b * sy)?;
                    self_tbl.set("c", c * sx)?;
                    self_tbl.set("d", d * sy)?;
                    Ok(LuaValue::Table(self_tbl))
                })?;
            t.set("scale", scale_fn)?;

            let rotate_fn = lua.create_function(|_, (self_tbl, angle): (LuaTable, f32)| {
                let cos = angle.cos();
                let sin = angle.sin();
                let a: f32 = self_tbl.get("a")?;
                let b: f32 = self_tbl.get("b")?;
                let c: f32 = self_tbl.get("c")?;
                let d: f32 = self_tbl.get("d")?;
                self_tbl.set("a", a * cos + b * sin)?;
                self_tbl.set("b", a * -sin + b * cos)?;
                self_tbl.set("c", c * cos + d * sin)?;
                self_tbl.set("d", c * -sin + d * cos)?;
                Ok(LuaValue::Table(self_tbl))
            })?;
            t.set("rotate", rotate_fn)?;

            let reset_fn = lua.create_function(|_, self_tbl: LuaTable| {
                self_tbl.set("a", 1.0f32)?;
                self_tbl.set("b", 0.0f32)?;
                self_tbl.set("c", 0.0f32)?;
                self_tbl.set("d", 1.0f32)?;
                self_tbl.set("tx", 0.0f32)?;
                self_tbl.set("ty", 0.0f32)?;
                Ok(LuaValue::Table(self_tbl))
            })?;
            t.set("reset", reset_fn)?;

            let transform_point_fn =
                lua.create_function(|_, (self_tbl, px, py): (LuaTable, f32, f32)| {
                    let a: f32 = self_tbl.get("a")?;
                    let b: f32 = self_tbl.get("b")?;
                    let c: f32 = self_tbl.get("c")?;
                    let d: f32 = self_tbl.get("d")?;
                    let tx: f32 = self_tbl.get("tx")?;
                    let ty: f32 = self_tbl.get("ty")?;
                    Ok((a * px + b * py + tx, c * px + d * py + ty))
                })?;
            t.set("transformPoint", transform_point_fn)?;

            let inv_transform_fn =
                lua.create_function(|_, (self_tbl, px, py): (LuaTable, f32, f32)| {
                    let a: f32 = self_tbl.get("a")?;
                    let b: f32 = self_tbl.get("b")?;
                    let c: f32 = self_tbl.get("c")?;
                    let d: f32 = self_tbl.get("d")?;
                    let tx: f32 = self_tbl.get("tx")?;
                    let ty: f32 = self_tbl.get("ty")?;
                    let det = a * d - b * c;
                    if det.abs() < 1e-10 {
                        return Ok((px as f64, py as f64));
                    }
                    let inv = 1.0 / det;
                    let ix = d * inv * (px - tx) + (-b * inv) * (py - ty);
                    let iy = (-c * inv) * (px - tx) + a * inv * (py - ty);
                    Ok((ix as f64, iy as f64))
                })?;
            t.set("inverseTransformPoint", inv_transform_fn)?;

            t.set(
                "clone",
                lua.create_function(|lua, self_tbl: LuaTable| {
                    let t2 = lua.create_table()?;
                    for pair in self_tbl.pairs::<LuaValue, LuaValue>() {
                        let (k, v) = pair?;
                        t2.set(k, v)?;
                    }
                    Ok(LuaValue::Table(t2))
                })?,
            )?;

            t.set(
                "type",
                lua.create_function(|_, _self: LuaValue| Ok("Transform"))?,
            )?;
            t.set(
                "typeOf",
                lua.create_function(|_, (_self, tp): (LuaValue, String)| {
                    Ok(tp == "Transform" || tp == "Object")
                })?,
            )?;

            Ok(LuaValue::Table(t))
        })?,
    )?;

    love.set("math", math_table)?;
    Ok(())
}

fn register_image(lua: &Lua, love: &LuaTable, _state: Arc<SharedState>) -> LuaResult<()> {
    let img = lua.create_table()?;

    // love.image.newImageData(width, height) or (filename)
    img.set(
        "newImageData",
        lua.create_function(|lua, args: LuaMultiValue| {
            let w = match args.get(0) {
                Some(LuaValue::Number(n)) => *n as u32,
                Some(LuaValue::Integer(n)) => *n as u32,
                _ => 1,
            };
            let h = match args.get(1) {
                Some(LuaValue::Number(n)) => *n as u32,
                Some(LuaValue::Integer(n)) => *n as u32,
                _ => 1,
            };
            let idata = lua.create_table()?;
            idata.set(
                "getWidth",
                lua.create_function(move |_, _self: LuaValue| Ok(w))?,
            )?;
            idata.set(
                "getHeight",
                lua.create_function(move |_, _self: LuaValue| Ok(h))?,
            )?;
            idata.set(
                "getDimensions",
                lua.create_function(move |_, _self: LuaValue| Ok((w, h)))?,
            )?;
            idata.set(
                "getPixel",
                lua.create_function(|_, (_self, _x, _y): (LuaValue, u32, u32)| {
                    Ok((0.0f64, 0.0f64, 0.0f64, 0.0f64))
                })?,
            )?;
            idata.set(
                "setPixel",
                lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
            )?;
            idata.set(
                "paste",
                lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
            )?;
            idata.set("release", lua.create_function(|_, _self: LuaValue| Ok(()))?)?;
            idata.set(
                "type",
                lua.create_function(|_, _self: LuaValue| Ok("ImageData"))?,
            )?;
            idata.set(
                "typeOf",
                lua.create_function(|_, (_self, t): (LuaValue, String)| {
                    Ok(t == "ImageData" || t == "Data" || t == "Object")
                })?,
            )?;
            Ok(LuaValue::Table(idata))
        })?,
    )?;

    // love.image.newCompressedData(filename)
    img.set(
        "newCompressedData",
        lua.create_function(|lua, _args: LuaMultiValue| {
            let cd = lua.create_table()?;
            cd.set(
                "getWidth",
                lua.create_function(|_, _self: LuaValue| Ok(1u32))?,
            )?;
            cd.set(
                "getHeight",
                lua.create_function(|_, _self: LuaValue| Ok(1u32))?,
            )?;
            cd.set(
                "type",
                lua.create_function(|_, _self: LuaValue| Ok("CompressedImageData"))?,
            )?;
            Ok(LuaValue::Table(cd))
        })?,
    )?;

    love.set("image", img)?;
    Ok(())
}
