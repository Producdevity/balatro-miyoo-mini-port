
local set_sfx = SET_SFX
function SET_SFX(source, args)
    if not string.find(source.sound_code, 'music') then return set_sfx(source, args) end
    local now = love.timer.getTime()
    local elapsed = source.music_updated_at and now - source.music_updated_at or (args.dt or 0)
    source.music_updated_at = now
    local dt = args.dt
    -- Keep the game's crossfade, but measure it independently of capped frame time.
    args.dt = (1 - math.exp(-3*math.max(0, elapsed)))/3
    set_sfx(source, args)
    args.dt = dt
end

function RESTART_MUSIC(args)
    local pending = {}
    for code, sources in pairs(SOURCES) do
        if string.find(code, 'music') then
            for _, source in ipairs(sources) do source.sound:stop() end
            SOURCES[code] = {}
            args.per, args.vol, args.sound_code = 0.7, 0.6, code
            local source = PLAY_SOUND(args, true)
            source.initialized = true
            pending[#pending + 1] = source.sound
        end
    end
    -- Submit all layers together after their files have been prepared.
    love.audio.play(pending)
end
