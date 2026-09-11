local phase, started, expected, initial_moves = 0, 0, nil, 0
local moves = 0

local function mark(label)
    started = love.timer.getTime()
    io.stderr:write('[controls-test] evdev ' .. label .. '\n')
    io.stderr:flush()
end

return function(frame)
    if phase == 0 then
        if G.STATE ~= G.STATES.SELECTING_HAND or G.CONTROLLER.locked then return end
        if not G.hand or #G.hand.cards < 5 then return end
        for _, card in ipairs(G.hand.cards) do if card.flipping then return end end
        _TUI_AUTOPLAY = false
        love.keypressed('miyoo_left', 'miyoo_left', false)
        love.keyreleased('miyoo_left', 'miyoo_left')
        expected = G.hand.cards[4]
        G.CONTROLLER:snap_to{node=expected}
        phase, started = 1, frame
    elseif phase == 1 and frame > started + 3 then
        assert(G.CONTROLLER.focused.target == expected, 'initial focus was lost')
        local navigate = G.CONTROLLER.navigate_focus
        G.CONTROLLER.navigate_focus = function(self, direction)
            if direction == 'L' then moves = moves + 1 end
            return navigate(self, direction)
        end
        phase = 2
        mark('ready for taps')
    elseif phase == 2 then
        if moves == 2 then
            assert(G.CONTROLLER.focused.target == G.hand.cards[2], 'two taps moved to the wrong card')
            phase = 3
            mark('ready for hold')
            return 'evdev-double-tap'
        end
        assert(moves < 2, 'tap burst repeated unexpectedly')
        assert(love.timer.getTime() - started < 8, 'raw double tap was lost')
    elseif phase == 3 then
        if moves == 3 then
            phase = 4
            mark('ready for repress')
        end
        assert(love.timer.getTime() - started < 8, 'raw held press was lost')
    elseif phase == 4 then
        if moves >= 5 then
            phase = 5
            mark('repeat observed')
        end
        assert(love.timer.getTime() - started < 8, 'release and repress stopped held repeat')
    elseif phase == 5 then
        if not G.CONTROLLER.held_buttons.dpleft then
            initial_moves, phase = moves, 6
            mark('released')
        end
        assert(love.timer.getTime() - started < 8, 'raw release did not arrive')
    elseif phase == 6 and love.timer.getTime() - started > 0.4 then
        assert(moves == initial_moves, 'navigation continued after release')
        phase = 7
        io.stderr:write('[controls-test] PASS: evdev double tap, quick repress, held repeat and release\n')
        return 'evdev-finished'
    end
end
