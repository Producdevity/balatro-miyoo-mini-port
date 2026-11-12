-- A, B or Start during the intro jumps to the main menu, as Escape does on desktop.
local tapped, done
return function(frame)
    if done then return end
    if not tapped then
        assert(frame < 600, 'intro never reached the splash screen')
        if G.STATE ~= G.STATES.SPLASH or frame < 20 then return end
        love.keypressed('miyoo_a', 'miyoo_a', false)
        love.keyreleased('miyoo_a', 'miyoo_a')
        tapped = frame
    elseif G.STATE == G.STATES.MENU then
        io.stderr:write(string.format('[controls-test] PASS: intro skipped after %d frames\n', frame - tapped))
        done = true
        return 'splash-skip'
    else
        assert(frame < tapped + 30, 'intro was not skipped')
    end
end
