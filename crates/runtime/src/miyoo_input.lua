local keymap = {
    w = 'dpup',
    s = 'dpdown',
    a = 'dpleft',
    d = 'dpright',
    up = 'dpup',
    down = 'dpdown',
    left = 'dpleft',
    right = 'dpright',
    space = 'a',
    ['return'] = 'a',
    escape = 'b',
    backspace = 'b',
    lshift = 'b',
    rshift = 'b',
    x = 'x',
    c = 'y',
    q = 'triggerleft',
    e = 'triggerright',
    tab = 'start',

    miyoo_up = 'dpup',
    miyoo_down = 'dpdown',
    miyoo_left = 'dpleft',
    miyoo_right = 'dpright',
    miyoo_a = 'a',
    miyoo_b = 'b',
    miyoo_x = 'x',
    miyoo_y = 'y',
    miyoo_l1 = 'leftshoulder',
    miyoo_r1 = 'rightshoulder',
    miyoo_l2 = 'triggerleft',
    miyoo_r2 = 'triggerright',
    miyoo_select = 'back',
    miyoo_start = 'start',
}

local held = {}
local gamepad = {
    getGamepadMappingString = function()
        return '00000000000000000000000000000000,Miyoo Mini,'
    end,
    getGamepadAxis = function() return 0 end,
    getName = function() return 'Miyoo Mini' end,
    isGamepadDown = function(_, ...)
        for i = 1, select('#', ...) do
            if held[select(i, ...)] then return true end
        end
        return false
    end,
}

local original_keypressed = love.keypressed
local original_keyreleased = love.keyreleased
local original_update = love.update
local trace_input = os.getenv('BALATRO_INPUT_TRACE') == '1'
local events, first, last = {}, 1, 0
local keys_down, button_counts = {}, {}

local function enqueue(button, pressed)
    last = last + 1
    events[last] = {button, pressed, love.timer.getTime()}
end

local function trace(kind, key, mapped)
    if not trace_input then return end
    io.stderr:write(string.format('[input] lua %s key=%s button=%s\n', kind, key, mapped))
    io.stderr:flush()
end

love.keypressed = function(key, scancode, isrepeat)
    if key == 'f10' or key == 'miyoo_menu' then
        love.event.quit()
        return
    end
    local mapped = keymap[key]
    if mapped and G and G.CONTROLLER then
        if isrepeat or keys_down[key] then return end
        keys_down[key] = true
        button_counts[mapped] = (button_counts[mapped] or 0) + 1
        trace('press', key, mapped)
        if button_counts[mapped] == 1 then enqueue(mapped, true) end
    elseif original_keypressed then
        original_keypressed(key, scancode, isrepeat)
    end
end

love.keyreleased = function(key, scancode)
    if key == 'f10' or key == 'miyoo_menu' then return end
    local mapped = keymap[key]
    if mapped and G and G.CONTROLLER then
        if not keys_down[key] then return end
        keys_down[key] = nil
        button_counts[mapped] = button_counts[mapped] - 1
        trace('release', key, mapped)
        if button_counts[mapped] == 0 then enqueue(mapped, false) end
    elseif original_keyreleased then
        original_keyreleased(key, scancode)
    end
end

local function dispatch(controller, dt)
    -- Run at the controller's input phase, after lock checks and before hover
    -- updates. Navigation can advance twice without drawing an intermediate frame.
    local pressed, navigation = false, false
    while first <= last do
        local event = events[first]
        local button, down = event[1], event[2]
        local direction = button == 'dpleft' or button == 'dpright' or
            button == 'dpup' or button == 'dpdown'
        if down and G.STATES and G.STATE == G.STATES.SPLASH and
            (button == 'a' or button == 'b' or button == 'start') then
            -- Desktop skips the intro on Escape or a click; the splash lock
            -- blocks the controller's own Start path.
            events[first] = nil
            first = first + 1
            G:delete_run()
            G:main_menu()
            break
        end
        if down and pressed and not (direction and navigation) then break end
        local expired = event[2] and event[4] and love.timer.getTime() - event[3] > 0.5
        local locks = controller.locks
        if event[2] and not expired and locks and locks.frame then
            event[4] = true
            break
        end
        events[first] = nil
        first = first + 1
        if expired then
            trace('expired', event[1], event[1])
        elseif down then
            held[button] = true
            love.gamepadpressed(gamepad, button)
            local mapped = G.button_mapping and G.button_mapping[button] or button
            controller.pressed_buttons[mapped] = nil
            if not G.screenwipe then controller:button_press_update(mapped, dt) end
            pressed, navigation = true, direction
        else
            held[button] = nil
            love.gamepadreleased(gamepad, button)
            local mapped = G.button_mapping and G.button_mapping[button] or button
            controller.released_buttons[mapped] = nil
            if not G.screenwipe then controller:button_release_update(mapped, dt) end
        end
    end
    if first > last then first, last = 1, 0 end
end

-- The game's held-direction repeat (0.3 s, then 0.1 s) counts its smoothed dt,
-- which it caps at 0.1 s. At handheld frame rates that makes repeats several
-- times slower than desktop. Feed held directions wall-clock time instead and
-- keep the remainder that the game's reset to zero would discard.
local hold_clock = {}
local function wall_clock_hold(controller)
    if controller.miyoo_wall_hold then return end
    controller.miyoo_wall_hold = true
    local original_hold = controller.button_hold_update
    local original_release = controller.button_release_update
    controller.button_hold_update = function(self, button, dt)
        if not (button == 'dpleft' or button == 'dpright' or
            button == 'dpup' or button == 'dpdown') then
            return original_hold(self, button, dt)
        end
        local now = love.timer.getTime()
        local elapsed = math.min(now - (hold_clock[button] or now - dt), 0.5)
        hold_clock[button] = now
        local before = self.held_button_times[button]
        local threshold = self.repress_timer or 0.3
        original_hold(self, button, elapsed)
        if before and before + elapsed > threshold and self.held_button_times[button] == 0 then
            self.held_button_times[button] = math.min(before + elapsed - threshold, 0.09)
        end
    end
    controller.button_release_update = function(self, button, dt)
        hold_clock[button] = nil
        return original_release(self, button, dt)
    end
end

love.update = function(dt)
    G.CONTROLLER.miyoo_dispatch = dispatch
    wall_clock_hold(G.CONTROLLER)
    if original_update then original_update(dt) end
end
