-- Use the game's controller update and click dispatch with a small scene.
-- Rendering and spatial navigation are covered by the on-device test instead.
selected, played, discarded, navigated = 0, 0, 0, 0
function EMPTY(t) for k in pairs(t) do t[k] = nil end; return t end
function Vector_Dist(a, b) return math.sqrt((a.x-b.x)^2 + (a.y-b.y)^2) end
G = {
    SETTINGS = {paused = false}, TIMERS = {TOTAL = 0},
    TILESIZE = 20, TILESCALE = 1, SPEEDFACTOR = 1, MIN_CLICK_DIST = 0.2,
    ROOM = {T = {w = 17, h = 13}},
    CURSOR = {T = {x = 1, y = 1}, VT = {x = 1, y = 1}, states = {}},
    I = {SPRITE = {}}, ARGS = {}, ASSET_ATLAS = {},
}
local node = {
    T = {x = 1, y = 1}, config = {},
    states = {click = {can = true}, drag = {can = false}, hover = {can = false}},
    click = function() selected = selected + 1 end,
}
G.CONTROLLER = Controller()
local controller = G.CONTROLLER
controller.focused.target = node
controller.cursor_position = {x = 20, y = 20}
controller.get_cursor_collision = function() end
controller.update_focus = function() end
controller.set_cursor_hover = function() end
controller.capture_focused_input = function() return false end
controller.navigate_focus = function() navigated = navigated + 1 end
controller.button_registry.x = {{node = {
    T = node.T, click = function() played = played + 1 end,
}, menu = false}}
controller.button_registry.y = {{node = {
    T = node.T, click = function() discarded = discarded + 1 end,
}, menu = false}}
love = {
    timer = {getTime = function() return G.TIMERS.TOTAL end},
    mouse = {setVisible = function() end},
    event = {quit = function() end},
    gamepadpressed = function(pad, button)
        controller:set_gamepad(pad)
        controller:set_HID_flags('button', button)
        controller:button_press(button)
    end,
    gamepadreleased = function(_, button) controller:button_release(button) end,
    update = function(dt)
        G.TIMERS.TOTAL = G.TIMERS.TOTAL + dt
        controller:update(dt)
    end,
}
