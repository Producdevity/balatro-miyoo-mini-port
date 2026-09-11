use mlua::prelude::*;

pub fn configure(lua: &Lua, frame: &LuaFunction) -> LuaResult<Option<String>> {
    let scope = std::env::var("BALATRO_JIT_SCOPE").unwrap_or_else(|_| "off".to_owned());
    if scope == "off" {
        return Ok(None);
    }
    if scope != "move"
        && scope != "minor-move"
        && scope != "scan"
        && scope != "scan-leaves"
        && scope != "scan-minor"
        && scope != "scan-ui"
        && scope != "scan-ui-leaves"
        && scope != "scan-minor-ui"
    {
        return Err(LuaError::external(format!(
            "unknown BALATRO_JIT_SCOPE: {scope}"
        )));
    }

    let globals = lua.globals();
    globals.set("_SVMM_JIT_FRAME", frame.clone())?;
    globals.set("_SVMM_JIT_SCOPE", scope.as_str())?;
    let result = lua
        .load(
            r#"
            local ok, loaded_jit = pcall(require, 'jit')
            if not ok or not loaded_jit then return 'unavailable' end

            loaded_jit.off()
            local seen = {}
            local function disable_functions(value, depth)
                if type(value) ~= 'table' or value == G or seen[value] or depth > 8 then return end
                seen[value] = true
                for _, child in pairs(value) do
                    if type(child) == 'function' then
                        pcall(loaded_jit.off, child, true)
                    elseif type(child) == 'table' then
                        disable_functions(child, depth + 1)
                    end
                end
            end

            pcall(loaded_jit.off, _SVMM_JIT_FRAME, true)
            disable_functions(_G, 0)

            local enabled = {}
            local scan = _SVMM_JIT_SCOPE == 'scan' or
                _SVMM_JIT_SCOPE == 'scan-leaves' or
                _SVMM_JIT_SCOPE == 'scan-minor' or
                _SVMM_JIT_SCOPE == 'scan-ui' or
                _SVMM_JIT_SCOPE == 'scan-ui-leaves' or
                _SVMM_JIT_SCOPE == 'scan-minor-ui'
            local leaves = _SVMM_JIT_SCOPE == 'scan-leaves' or
                _SVMM_JIT_SCOPE == 'scan-minor' or
                _SVMM_JIT_SCOPE == 'scan-ui' or
                _SVMM_JIT_SCOPE == 'scan-ui-leaves' or
                _SVMM_JIT_SCOPE == 'scan-minor-ui'
            if scan and SVMM_collect_moveables then
                loaded_jit.on(SVMM_collect_moveables)
                enabled[#enabled + 1] = 'SVMM_collect_moveables'
                for name, fn in pairs(SVMM_MOVE_SCAN_JIT_FUNCTIONS or {}) do
                    if type(fn) == 'function' then
                        loaded_jit.on(fn)
                        enabled[#enabled + 1] = 'scan.' .. name
                    end
                end
            end
            if leaves and Moveable then
                local names = {
                    'move_juice', 'move_xy', 'move_r', 'move_scale', 'move_wh',
                    'calculate_parrallax'
                }
                for _, name in ipairs(names) do
                    local fn = Moveable[name]
                    if type(fn) == 'function' then
                        loaded_jit.on(fn)
                        enabled[#enabled + 1] = name
                    end
                end
            end
            if leaves then
                for name, fn in pairs(SVMM_UPDATE_JIT_FUNCTIONS or {}) do
                    if type(fn) == 'function' then
                        loaded_jit.on(fn)
                        enabled[#enabled + 1] = 'update.' .. name
                    end
                end
            end
            if (_SVMM_JIT_SCOPE == 'minor-move' or
                _SVMM_JIT_SCOPE == 'scan-minor' or
                _SVMM_JIT_SCOPE == 'scan-minor-ui') and Moveable then
                local names = {
                    'move_with_major', 'get_major', 'move_juice', 'move_xy',
                    'move_r', 'move_scale', 'move_wh', 'calculate_parrallax'
                }
                for _, name in ipairs(names) do
                    local fn = Moveable[name]
                    if type(fn) == 'function' then
                        loaded_jit.on(fn)
                        enabled[#enabled + 1] = name
                    end
                end
            end
            if (_SVMM_JIT_SCOPE == 'scan-ui' or
                _SVMM_JIT_SCOPE == 'scan-minor-ui') and UIElement then
                loaded_jit.on(UIElement.draw_self)
                enabled[#enabled + 1] = 'UIElement.draw_self'
                for name, fn in pairs(SVMM_UI_JIT_FUNCTIONS or {}) do
                    if type(fn) == 'function' then
                        loaded_jit.on(fn)
                        enabled[#enabled + 1] = 'ui.' .. name
                    end
                end
            end
            if _SVMM_JIT_SCOPE == 'scan-ui-leaves' then
                local functions = SVMM_UI_JIT_FUNCTIONS or {}
                for _, name in ipairs({'draw_polygon', 'draw_text'}) do
                    local fn = functions[name]
                    if type(fn) == 'function' then
                        loaded_jit.on(fn)
                        enabled[#enabled + 1] = 'ui.' .. name
                    end
                end
            end
            if _SVMM_JIT_SCOPE == 'move' and Moveable then
                local names = {
                    'move', 'align_to_major', 'glue_to_major', 'move_with_major',
                    'move_juice', 'move_xy', 'move_r', 'move_scale', 'move_wh',
                    'calculate_parrallax', 'get_major', 'lr_clamp'
                }
                for _, name in ipairs(names) do
                    local fn = Moveable[name]
                    if type(fn) == 'function' then
                        loaded_jit.on(fn)
                        enabled[#enabled + 1] = name
                    end
                end
            end

            require('jit.opt').start('hotloop=30', 'hotexit=10', 'maxtrace=128')
            loaded_jit.on()
            return table.concat(enabled, ',')
            "#,
        )
        .set_name("@selective_jit.lua")
        .eval()?;
    globals.set("_SVMM_JIT_FRAME", LuaNil)?;
    globals.set("_SVMM_JIT_SCOPE", LuaNil)?;
    Ok(Some(result))
}
