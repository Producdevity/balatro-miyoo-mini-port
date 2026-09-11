G.F_ENABLE_PERF_OVERLAY = true
G.check = {
    draw = {checkpoint_list = {}, checkpoints = 0, last_time = 0},
    update = {checkpoint_list = {}, checkpoints = 0, last_time = 0}
}
function timer_checkpoint(label, kind, reset)
    local checkpoints = G.check[kind]
    local now = love.timer.getTime()
    if reset then
        checkpoints.last_time = now
        checkpoints.checkpoints = 0
        return
    end
    local elapsed = now - checkpoints.last_time
    local index = checkpoints.checkpoints + 1
    local item = checkpoints.checkpoint_list[index]
    if not item then
        item = {label = label, average = elapsed}
        checkpoints.checkpoint_list[index] = item
    else
        item.label = label
        item.average = item.average * 0.875 + elapsed * 0.125
    end
    checkpoints.checkpoints = index
    checkpoints.last_time = now
end

if _SVMM_PROFILE_OWNERS then
G.SVMM_DRAW_PROFILE = {}
local function profile_draw(class, method, label, identify)
    local saved = '_svmm_profile_' .. method
    if not class or class[saved] then return end
    class[saved] = class[method]
    class[method] = function(self, ...)
        if not _SVMM_PROFILE_SAMPLE then return class[saved](self, ...) end
        local started = love.timer.getTime()
        local result = class[saved](self, ...)
        local key = label
        if identify then key = key .. ':' .. identify(self) end
        local entry = G.SVMM_DRAW_PROFILE[key]
        if not entry then
            entry = {calls = 0, elapsed = 0}
            G.SVMM_DRAW_PROFILE[key] = entry
        end
        entry.calls = entry.calls + 1
        entry.elapsed = entry.elapsed + love.timer.getTime() - started
        return result
    end
end
profile_draw(UIBox, 'draw', 'UIBox', function(box)
    local root = box.UIRoot and box.UIRoot.config or {}
    local config = box.config or {}
    local name
    if box == G.HUD then
        name = 'HUD'
    elseif box == G.HUD_blind then
        name = 'HUD_blind'
    else
        name = config.instance_type or config.id or root.id or '?'
    end
    return string.format('%s@%.1fx%.1f', tostring(name), box.T.w or 0, box.T.h or 0)
end)
profile_draw(CardArea, 'draw', 'CardArea')
profile_draw(Card, 'draw', 'Card')
profile_draw(UIElement, 'draw_self', 'UIElement.draw_self')

G.SVMM_MOVE_PROFILE = {}
local function profile_move(class, label)
    if not class or rawget(class, '_svmm_profile_move') then return end
    class._svmm_profile_move = class.move
    class.move = function(self, ...)
        if not _SVMM_PROFILE_SAMPLE then return class._svmm_profile_move(self, ...) end
        local started = love.timer.getTime()
        local result = class._svmm_profile_move(self, ...)
        local key = label
        if label == 'Moveable' then
            key = label .. ':' .. tostring(self.role and self.role.role_type or '?')
        end
        local entry = G.SVMM_MOVE_PROFILE[key]
        if not entry then
            entry = {calls = 0, elapsed = 0}
            G.SVMM_MOVE_PROFILE[key] = entry
        end
        entry.calls = entry.calls + 1
        entry.elapsed = entry.elapsed + love.timer.getTime() - started
        return result
    end
end
profile_move(Moveable, 'Moveable')
profile_move(CardArea, 'CardArea')

G.SVMM_UPDATE_PROFILE = {}
local function profile_update(class, method, label)
    local original = class and class[method]
    if not original then return end
    class[method] = function(self, ...)
        if not _SVMM_PROFILE_SAMPLE then return original(self, ...) end
        local started = love.timer.getTime()
        local result = original(self, ...)
        local entry = G.SVMM_UPDATE_PROFILE[label]
        if not entry then
            entry = {calls = 0, elapsed = 0}
            G.SVMM_UPDATE_PROFILE[label] = entry
        end
        entry.calls = entry.calls + 1
        entry.elapsed = entry.elapsed + love.timer.getTime() - started
        return result
    end
end
profile_update(Card, 'update', 'Card')
profile_update(CardArea, 'update', 'CardArea')
profile_update(CardArea, 'align_cards', 'CardArea.align_cards')
profile_update(UIElement, 'update', 'UIElement')
profile_update(DynaText, 'update', 'DynaText')
profile_update(Controller, 'get_cursor_collision', 'CursorCollision')

local jit_ok, loaded_jit = pcall(require, 'jit')
if jit_ok and loaded_jit then
    local jutil_ok, jutil = pcall(require, 'jit.util')
    G.SVMM_JIT_PROFILE = {started = 0, stopped = 0, aborted = 0, reasons = {}}
    loaded_jit.attach(function(what, _, fn, pc, reason, detail)
        local profile = G.SVMM_JIT_PROFILE
        if what == 'start' then
            profile.started = profile.started + 1
        elseif what == 'stop' then
            profile.stopped = profile.stopped + 1
        elseif what == 'abort' then
            profile.aborted = profile.aborted + 1
            local where = '?'
            if jutil_ok and jutil then
                local ok_info, info = pcall(jutil.funcinfo, fn, pc)
                if ok_info and info then where = tostring(info.loc or '?') end
            elseif type(fn) == 'function' and debug and debug.getinfo then
                local ok_info, info = pcall(debug.getinfo, fn, 'Sl')
                if ok_info and info then
                    where = tostring(info.short_src or info.source or '?') .. ':' ..
                        tostring(info.linedefined or 0)
                end
            end
            local key = tostring(reason) .. ':' .. tostring(detail) .. '@' .. where
            profile.reasons[key] = (profile.reasons[key] or 0) + 1
        end
    end, 'trace')
end
end
