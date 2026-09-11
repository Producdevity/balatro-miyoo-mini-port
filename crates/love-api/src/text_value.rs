use mlua::prelude::*;

use crate::lua_util::color_f32_to_u8;

pub(crate) type ColoredSegment = ([u8; 4], String);

fn text_string(lua: &Lua, value: &LuaValue) -> LuaResult<String> {
    lua.coerce_string(value.clone())?
        .map(|text| text.to_string_lossy())
        .ok_or_else(|| LuaError::FromLuaConversionError {
            from: value.type_name(),
            to: "text".to_owned(),
            message: Some("expected a string or number".to_owned()),
        })
}

pub(crate) fn extract_text_from_lua(lua: &Lua, value: &LuaValue) -> LuaResult<String> {
    match value {
        LuaValue::Nil => Ok(String::new()),
        LuaValue::Table(table) => {
            let mut text = String::new();
            for i in 1..=table.raw_len() {
                let value = table.raw_get::<LuaValue>(i)?;
                if !matches!(value, LuaValue::Table(_)) {
                    text.push_str(&text_string(lua, &value)?);
                }
            }
            Ok(text)
        }
        _ => text_string(lua, value),
    }
}

pub(crate) fn parse_colored_text(
    lua: &Lua,
    value: &LuaValue,
) -> LuaResult<Option<Vec<ColoredSegment>>> {
    let LuaValue::Table(table) = value else {
        return Ok(None);
    };
    let mut segments = Vec::new();
    let mut color = [255; 4];
    for i in 1..=table.raw_len() {
        match table.raw_get::<LuaValue>(i)? {
            LuaValue::Table(channels) => {
                color = color_f32_to_u8([
                    channels.raw_get::<f32>(1)?,
                    channels.raw_get::<f32>(2)?,
                    channels.raw_get::<f32>(3)?,
                    channels.raw_get::<Option<f32>>(4)?.unwrap_or(1.0),
                ]);
            }
            value => {
                let text = text_string(lua, &value)?;
                if !text.is_empty() {
                    segments.push((color, text));
                }
            }
        }
    }
    Ok((!segments.is_empty()).then_some(segments))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colored_text_keeps_numeric_counts_and_colours() {
        let lua = Lua::new();
        let value = lua
            .load("return {{0, 0, 1}, 3, ' hands', {1, 0, 0, 0.5}, -2}")
            .eval::<LuaValue>()
            .unwrap();
        assert_eq!(
            parse_colored_text(&lua, &value).unwrap().unwrap(),
            vec![
                ([0, 0, 255, 255], "3".to_owned()),
                ([0, 0, 255, 255], " hands".to_owned()),
                ([255, 0, 0, 127], "-2".to_owned()),
            ]
        );
        assert_eq!(extract_text_from_lua(&lua, &value).unwrap(), "3 hands-2");
    }

    #[test]
    fn numeric_text_uses_lua_formatting() {
        let lua = Lua::new();
        let tostring = lua.globals().get::<LuaFunction>("tostring").unwrap();
        for value in [0.0, -0.0, 3.0, -2.25, 1e-9, 1e20] {
            let value = LuaValue::Number(value);
            let expected = tostring.call::<String>(value.clone()).unwrap();
            assert_eq!(extract_text_from_lua(&lua, &value).unwrap(), expected);
        }
    }

    #[test]
    fn empty_text_is_allowed_but_invalid_values_are_not_silently_dropped() {
        let lua = Lua::new();
        assert_eq!(extract_text_from_lua(&lua, &LuaValue::Nil).unwrap(), "");
        let empty = lua
            .load("return {{1, 1, 1}, ''}")
            .eval::<LuaValue>()
            .unwrap();
        assert!(parse_colored_text(&lua, &empty).unwrap().is_none());
        let invalid = lua
            .load("return {{1, 1, 1}, true}")
            .eval::<LuaValue>()
            .unwrap();
        assert!(parse_colored_text(&lua, &invalid).is_err());
    }
}
