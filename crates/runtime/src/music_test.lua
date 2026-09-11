local targets = {'music4', 'music3', 'music5', 'music1'}
local index, started = 0, nil
return function(frame)
    if frame == 60 then
        _TUI_AUTOPLAY = false
        G:main_menu()
    end
    if frame < 100 or index > #targets then return end
    if frame == 100 then
        -- Reproduce storage latency between layers without stalling the mixer.
        local new_source = love.audio.newSource
        love.audio.newSource = function(path, ...)
            if string.find(path, '/music') then love.timer.sleep(0.08) end
            return new_source(path, ...)
        end
        RESTART_MUSIC{desired_track='music1', dt=0.016, pitch_mod=1,
            state=G.STATE, sound_settings=G.SETTINGS.SOUND}
        love.audio.newSource = new_source
    end
    if not started then
        index = index + 1
        if index > #targets then
            G.video_soundtrack = nil
            io.stderr:write('[controls-test] PASS: four low-frame-rate music crossfades\n')
            return 'music-finished'
        end
        G.video_soundtrack = targets[index]
        started = love.timer.getTime()
    end
    if love.timer.getTime() - started < 1.5 then return end
    local tracks = 0
    for name, sources in pairs(SOURCES) do
        if string.find(name, 'music') then
            assert(#sources == 1, 'duplicate or missing music source: ' .. name)
            local source = sources[1]
            assert(source.sound:isPlaying(), 'music stopped: ' .. name)
            if name == targets[index] then
                assert(source.current_volume > 0.98, 'new music did not fade in')
            else
                assert(source.current_volume < 0.02, 'old music did not fade out: ' .. name)
            end
            assert(math.abs(source.sound:getPitch() - 0.7) < 0.0001, 'music pitch changed')
            tracks = tracks + 1
        end
    end
    assert(tracks == 5, 'music track set is incomplete')
    io.stderr:write(string.format('[music-test] target=%s elapsed=%.3f tracks=%d\n',
        targets[index], love.timer.getTime() - started, tracks))
    started = nil
end
