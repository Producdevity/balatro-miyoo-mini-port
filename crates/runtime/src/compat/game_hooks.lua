local _orig_require = require
local _tui_game_hooked = false
require = function(modname, ...)
    local result = _orig_require(modname, ...)
    if not _tui_game_hooked and type(Game) == "table" then
        _tui_game_hooked = true
        local _orig_srs = Game.set_render_settings
        if _orig_srs then
            Game.set_render_settings = function(self, ...)
                if self.SETTINGS and self.SETTINGS.GRAPHICS then
                    self.SETTINGS.GRAPHICS.texture_scaling = 1
                end
                return _orig_srs(self, ...)
            end
        end
    end
    return result
end
