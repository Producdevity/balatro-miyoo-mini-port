use mlua::prelude::*;
use parking_lot::Mutex;
use sprite_to_text::pixel_buffer::CardShaderInputs;
use std::sync::Arc;

pub type SharedInputs = Arc<Mutex<CardShaderInputs>>;

struct Uniforms(SharedInputs);
impl LuaUserData for Uniforms {}

pub(crate) fn register(lua: &Lua, shader: &LuaTable) -> LuaResult<()> {
    shader.set(
        "_software_inputs",
        lua.create_userdata(Uniforms(Arc::new(Mutex::new(CardShaderInputs::default()))))?,
    )
}

pub(crate) fn shared(shader: &LuaTable) -> LuaResult<SharedInputs> {
    let data: LuaAnyUserData = shader.get("_software_inputs")?;
    Ok(Arc::clone(&data.borrow::<Uniforms>()?.0))
}

fn vector_name(effect: u8) -> Option<&'static str> {
    [
        "",
        "played",
        "debuff",
        "foil",
        "holo",
        "polychrome",
        "negative",
        "voucher",
        "booster",
        "hologram",
        "negative_shine",
        "gold_seal",
    ]
    .get(usize::from(effect))
    .copied()
    .filter(|name| !name.is_empty())
}

pub(crate) fn send(shader: &LuaTable, name: &str, value: &LuaValue) -> LuaResult<()> {
    let effect = shader.get::<u8>("_card_shader")?;
    if name == "time" {
        let seed = match value {
            LuaValue::Number(value) => *value as f32,
            LuaValue::Integer(value) => *value as f32,
            _ => {
                return Err(LuaError::RuntimeError(
                    "shader time must be a number".into(),
                ))
            }
        };
        shared(shader)?.lock().seed = seed;
    } else if vector_name(effect) == Some(name) {
        let LuaValue::Table(values) = value else {
            return Err(LuaError::RuntimeError(
                "card shader value must be a table".into(),
            ));
        };
        let phase = values.get::<f32>(1)?;
        let clock = values.get::<f32>(2)?;
        let inputs = shared(shader)?;
        let mut inputs = inputs.lock();
        inputs.phase = phase;
        inputs.clock = clock;
    }
    Ok(())
}

pub(crate) fn update_fast(
    shader: &LuaTable,
    seed: f32,
    values: Option<LuaTable>,
) -> LuaResult<SharedInputs> {
    // Copy the numbers now: the game reuses and mutates its Lua send table.
    let vector = values
        .map(|values| Ok::<_, LuaError>((values.get::<f32>(1)?, values.get::<f32>(2)?)))
        .transpose()?;
    let inputs = shared(shader)?;
    {
        let mut inputs = inputs.lock();
        inputs.seed = seed;
        if let Some((phase, clock)) = vector {
            inputs.phase = phase;
            inputs.clock = clock;
        }
    }
    Ok(inputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniforms_are_copied_and_remain_local_to_each_shader() -> LuaResult<()> {
        let lua = Lua::new();
        let holo = lua.create_table()?;
        let foil = lua.create_table()?;
        for (shader, effect) in [(&holo, 4), (&foil, 3)] {
            shader.set("_card_shader", effect)?;
            register(&lua, shader)?;
        }
        assert_eq!(*shared(&holo)?.lock(), CardShaderInputs::default());
        let values = lua.create_sequence_from([1.25_f32, 28.0])?;
        let active = update_fast(&holo, 99.5, Some(values.clone()))?;
        values.set(1, 7.0)?;
        assert_eq!(active.lock().phase, 1.25);
        update_fast(&foil, 12.0, Some(values.clone()))?;
        update_fast(&holo, 100.0, None)?;
        assert_eq!(
            *active.lock(),
            CardShaderInputs {
                phase: 1.25,
                clock: 28.0,
                seed: 100.0
            }
        );
        send(&holo, "holo", &LuaValue::Table(values))?;
        send(&holo, "time", &LuaValue::Number(42.0))?;
        assert_eq!(
            *active.lock(),
            CardShaderInputs {
                phase: 7.0,
                clock: 28.0,
                seed: 42.0
            }
        );
        assert_eq!(shared(&foil)?.lock().seed, 12.0);
        Ok(())
    }
}
