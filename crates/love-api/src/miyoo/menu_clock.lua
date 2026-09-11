
local event_init = Event.init
function Event:init(config)
    event_init(self, config)
    if self.created_on_pause and self.timer == 'REAL' and not config.timer then
        self.timer = 'MENU'
        G.TIMERS.MENU = love.timer.getTime()
        self.time = G.TIMERS.MENU
    end
end

local manager_update = EventManager.update
function EventManager:update(...)
    -- Menu transitions use elapsed time, not the game's capped frame delta.
    G.TIMERS.MENU = love.timer.getTime()
    return manager_update(self, ...)
end
