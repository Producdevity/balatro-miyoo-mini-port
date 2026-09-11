use mlua::prelude::*;

const PATCH: &str = include_str!("miyoo_input.lua");

pub fn install(lua: &Lua) -> LuaResult<()> {
    if let Some(controller) = lua.globals().get::<Option<LuaTable>>("Controller")? {
        if !controller
            .get::<Option<bool>>("miyoo_input_phase")?
            .unwrap_or(false)
        {
            return Err(LuaError::runtime("Game controller input hook is missing"));
        }
    }
    if lua
        .globals()
        .get::<Option<LuaTable>>("UIElement")?
        .is_some()
    {
        lua.load(include_str!("miyoo_navigation.lua"))
            .set_name("@miyoo/navigation.lua")
            .exec()?;
    }
    lua.load(PATCH).set_name("@miyoo/input.lua").exec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_game_script(
        lua: &Lua,
        game: &love_api::state::GameSource,
        name: &str,
    ) -> LuaResult<()> {
        lua.load(game.read_file(name).unwrap())
            .set_name(name)
            .exec()
    }

    fn test_lua() -> LuaResult<Lua> {
        let lua = Lua::new();
        lua.load(
            r#"
            pressed = {}
            released = {}
            quit_count = 0
            fallback_count = 0
            love = {
                timer = {getTime = function() return 0 end},
                event = {quit = function() quit_count = quit_count + 1 end},
                keypressed = function() fallback_count = fallback_count + 1 end,
                keyreleased = function() fallback_count = fallback_count + 1 end,
                gamepadpressed = function(_, button) pressed[#pressed + 1] = button end,
                gamepadreleased = function(_, button) released[#released + 1] = button end,
            }
            G = {CONTROLLER = {
                pressed_buttons = {}, released_buttons = {},
                button_press_update = function() end,
                button_release_update = function() end,
            }}
            love.update = function(dt) G.CONTROLLER:miyoo_dispatch(dt) end
            "#,
        )
        .exec()?;
        install(&lua)?;
        Ok(lua)
    }

    #[test]
    fn missing_controller_hook_fails_instead_of_silently_losing_input() -> LuaResult<()> {
        let lua = Lua::new();
        lua.load("Controller = {}").exec()?;
        assert!(install(&lua)
            .unwrap_err()
            .to_string()
            .contains("input hook is missing"));
        Ok(())
    }

    #[test]
    fn maps_every_miyoo_control_to_balatro() -> LuaResult<()> {
        let lua = test_lua()?;
        let keypressed: LuaFunction = lua.globals().get::<LuaTable>("love")?.get("keypressed")?;
        let keyreleased: LuaFunction = lua.globals().get::<LuaTable>("love")?.get("keyreleased")?;
        let expected = [
            ("miyoo_up", "dpup"),
            ("miyoo_down", "dpdown"),
            ("miyoo_left", "dpleft"),
            ("miyoo_right", "dpright"),
            ("miyoo_a", "a"),
            ("miyoo_b", "b"),
            ("miyoo_x", "x"),
            ("miyoo_y", "y"),
            ("miyoo_l1", "leftshoulder"),
            ("miyoo_r1", "rightshoulder"),
            ("miyoo_l2", "triggerleft"),
            ("miyoo_r2", "triggerright"),
            ("miyoo_select", "back"),
            ("miyoo_start", "start"),
        ];

        for (key, _) in expected {
            keypressed.call::<()>((key, key, false))?;
            keyreleased.call::<()>((key, key))?;
            let update: LuaFunction = lua.globals().get::<LuaTable>("love")?.get("update")?;
            update.call::<()>(0.05)?;
        }

        let pressed: LuaTable = lua.globals().get("pressed")?;
        let released: LuaTable = lua.globals().get("released")?;
        for (index, (_, button)) in expected.into_iter().enumerate() {
            assert_eq!(pressed.get::<String>(index + 1)?, button);
            assert_eq!(released.get::<String>(index + 1)?, button);
        }
        Ok(())
    }

    #[test]
    fn menu_quits_and_unknown_keys_reach_the_game() -> LuaResult<()> {
        let lua = test_lua()?;
        let keypressed: LuaFunction = lua.globals().get::<LuaTable>("love")?.get("keypressed")?;
        keypressed.call::<()>(("miyoo_menu", "miyoo_menu", false))?;
        keypressed.call::<()>(("unknown", "unknown", false))?;

        assert_eq!(lua.globals().get::<u32>("quit_count")?, 1);
        assert_eq!(lua.globals().get::<u32>("fallback_count")?, 1);
        Ok(())
    }

    #[test]
    fn held_buttons_ignore_repeat_and_handle_alias_keys() -> LuaResult<()> {
        let lua = test_lua()?;
        lua.load(
            r#"
            local pad
            love.gamepadpressed = function(p) pad = p end
            love.keypressed('miyoo_a', 'miyoo_a', false)
            love.keypressed('miyoo_a', 'miyoo_a', true)
            love.keypressed('space', 'space', false)
            love.update(0.05)
            assert(pad:isGamepadDown('a'))
            assert(pad:isGamepadDown('b', 'a'))
            assert(not pad:isGamepadDown('b'))
            love.keyreleased('miyoo_a', 'miyoo_a')
            love.update(0.05)
            assert(pad:isGamepadDown('a'))
            love.keyreleased('space', 'space')
            love.update(0.05)
            assert(not pad:isGamepadDown('a'))
            assert(#released == 1, 'release was duplicated')
        "#,
        )
        .exec()
    }

    #[test]
    fn brief_menu_locks_preserve_taps_but_old_presses_expire() -> LuaResult<()> {
        let lua = test_lua()?;
        lua.load(
            r#"
            local now = 0
            love.timer.getTime = function() return now end
            G.CONTROLLER.locks = {frame=true}
            love.keypressed('miyoo_left', 'miyoo_left', false)
            love.keyreleased('miyoo_left', 'miyoo_left')
            love.keypressed('miyoo_left', 'miyoo_left', false)
            love.keyreleased('miyoo_left', 'miyoo_left')
            for i=1,4 do now=now+0.03; love.update(0.03) end
            assert(#pressed == 0, 'input bypassed the menu lock')
            G.CONTROLLER.locks.frame = nil
            love.update(0.03)
            love.update(0.03)
            assert(#pressed == 2 and #released == 2, 'queued taps were lost')
            G.CONTROLLER.locks.frame = true
            love.keypressed('miyoo_a', 'miyoo_a', false)
            love.keyreleased('miyoo_a', 'miyoo_a')
            now = now + 10
            love.update(0.03)
            G.CONTROLLER.locks.frame = nil
            love.update(0.03)
            assert(#pressed == 2, 'old action replayed after a long pause')
            love.keypressed('miyoo_left', 'miyoo_left', false)
            love.keyreleased('miyoo_left', 'miyoo_left')
            now = now + 1
            love.update(0.03)
            assert(#pressed == 3, 'slow rendering discarded an unlocked press')
        "#,
        )
        .exec()
    }

    #[test]
    #[ignore = "requires user-owned game archive in BALATRO_TEST_GAME"]
    fn controller_page_changes_do_not_use_mouse_debounce() -> LuaResult<()> {
        let path = std::env::var("BALATRO_TEST_GAME").expect("set BALATRO_TEST_GAME");
        let game = love_api::state::GameSource::from_path(std::path::Path::new(&path)).unwrap();
        let lua = Lua::new();
        load_game_script(&lua, &game, "engine/object.lua")?;
        lua.load("Moveable = Object:extend()").exec()?;
        for name in ["engine/controller.lua", "engine/ui.lua"] {
            load_game_script(&lua, &game, name)?;
        }
        lua.load(include_str!("controller_test_fixture.lua"))
            .exec()?;
        lua.load(
            r#"
            page_changes = 0
            G.TIMERS.REAL = 1
            G.ROOM.jiggle = 0
            G.VIBRATION = 0
            G.FUNCS = {option_cycle=function() page_changes=page_changes+1 end}
            play_sound = function() end
            arrow = setmetatable({config={button='option_cycle'}, states={visible=true}}, UIElement)
            local focused = G.CONTROLLER.focused.target
            focused.config.focus_args = {type='cycle'}
            focused.children = {arrow, {}, arrow}
            G.CONTROLLER.capture_focused_input = Controller.capture_focused_input
            G.CONTROLLER:capture_focused_input('dpleft', 'press', 0.025)
            G.CONTROLLER:capture_focused_input('dpleft', 'press', 0.025)
            assert(page_changes == 1, 'test did not reproduce the original debounce')
            page_changes, arrow.last_clicked = 0, nil
        "#,
        )
        .exec()?;
        install(&lua)?;
        lua.load(
            r#"
            G.CONTROLLER.capture_focused_input = Controller.capture_focused_input
            for i=1,2 do
                love.keypressed('miyoo_left', 'miyoo_left', false)
                love.keyreleased('miyoo_left', 'miyoo_left')
            end
            love.update(0.025)
            assert(page_changes == 2, 'rapid page change was lost')
            arrow:click()
            assert(page_changes == 2, 'mouse debounce was disabled')
            arrow.disable_button = true
            G.CONTROLLER:capture_focused_input('dpleft', 'press', 0.025)
            assert(page_changes == 2, 'disabled button accepted input')
            arrow.disable_button = false
            arrow.config.one_press = true
            G.CONTROLLER:capture_focused_input('dpleft', 'press', 0.025)
            G.CONTROLLER:capture_focused_input('dpleft', 'press', 0.025)
            assert(page_changes == 3, 'one-press protection was removed')
        "#,
        )
        .exec()
    }

    #[test]
    #[ignore = "requires user-owned game archive in BALATRO_TEST_GAME"]
    fn original_button_tables_lose_repeat_after_a_quick_repress() -> LuaResult<()> {
        let path = std::env::var("BALATRO_TEST_GAME").expect("set BALATRO_TEST_GAME");
        let game = love_api::state::GameSource::from_path(std::path::Path::new(&path)).unwrap();
        let lua = Lua::new();
        for name in ["engine/object.lua", "engine/controller.lua"] {
            load_game_script(&lua, &game, name)?;
        }
        lua.load(include_str!("controller_test_fixture.lua"))
            .exec()?;
        lua.load(r#"
            local pad = G.CONTROLLER.keyboard_controller
            love.gamepadpressed(pad, 'dpleft')
            love.update(0.05)
            love.gamepadreleased(pad, 'dpleft')
            love.gamepadpressed(pad, 'dpleft')
            love.update(0.05)
            assert(navigated == 2, 'initial presses must both arrive')
            assert(G.CONTROLLER.held_buttons.dpleft, 'button must still be down')
            assert(G.CONTROLLER.held_button_times.dpleft == nil, 'expected the stale release to clear the new hold')
            for i = 1, 10 do love.update(0.05) end
            assert(navigated == 2, 'test did not reproduce the lost repeat')
        "#).exec()
    }

    #[test]
    #[ignore = "requires user-owned game archive in BALATRO_TEST_GAME"]
    fn queued_taps_reach_the_original_controller() -> LuaResult<()> {
        let path = std::env::var("BALATRO_TEST_GAME").expect("set BALATRO_TEST_GAME");
        let game = love_api::state::GameSource::from_path(std::path::Path::new(&path)).unwrap();
        let lua = Lua::new();
        for name in ["engine/object.lua", "engine/controller.lua"] {
            load_game_script(&lua, &game, name)?;
        }
        lua.load(include_str!("controller_test_fixture.lua"))
            .exec()?;
        install(&lua)?;
        lua.load(
            r#"
            local function tap(key)
                love.keypressed(key, key, false)
                love.keyreleased(key, key)
            end
            tap('miyoo_a')
            tap('miyoo_a')
            for i = 1, 4 do love.update(0.05) end
            assert(selected == 2, 'two taps selected ' .. selected .. ' times')
            assert(not G.CONTROLLER.is_cursor_down, 'cursor stayed pressed')

            tap('miyoo_x')
            tap('miyoo_y')
            for i = 1, 4 do love.update(0.05) end
            assert(played == 1 and discarded == 1, 'one of the action buttons was lost')

            tap('miyoo_right')
            tap('miyoo_a')
            for i = 1, 4 do love.update(0.05) end
            assert(navigated == 1 and selected == 3, 'navigation swallowed selection')

            love.keypressed('miyoo_right', 'miyoo_right', false)
            for i = 1, 20 do love.update(0.05) end
            assert(navigated > 2, 'held D-pad did not repeat')
            love.keyreleased('miyoo_right', 'miyoo_right')
            love.update(0.05)
            local stopped = navigated
            for i = 1, 10 do love.update(0.05) end
            assert(navigated == stopped, 'D-pad kept repeating after release')

            tap('miyoo_left')
            tap('miyoo_left')
            love.update(0.05)
            assert(navigated == stopped + 2, 'two taps waited for a second game frame')

            love.keypressed('miyoo_left', 'miyoo_left', false)
            love.update(0.05)
            love.keyreleased('miyoo_left', 'miyoo_left')
            love.keypressed('miyoo_left', 'miyoo_left', false)
            love.update(0.05)
            local repressed = navigated
            for i = 1, 10 do love.update(0.05) end
            assert(navigated > repressed, 'quick release and re-press disabled held repeat')
            love.keyreleased('miyoo_left', 'miyoo_left')
            love.update(0.05)

            G.screenwipe = true
            tap('miyoo_a')
            love.update(0.05)
            G.screenwipe = nil
            for i = 1, 3 do love.update(0.05) end
            assert(selected == 3, 'an action replayed after the screen wipe')
        "#,
        )
        .exec()
    }
}
