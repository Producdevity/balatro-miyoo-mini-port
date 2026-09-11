local function tap(key)
    love.keypressed('miyoo_' .. key, 'miyoo_' .. key, false)
    love.keyreleased('miyoo_' .. key, 'miyoo_' .. key)
end

local phase, started, expected, moves = 0, 0, nil, 0
local page_changes, page_checks = 0, 0
return function(frame)
    if phase == 0 then
        if G.STATE ~= G.STATES.SELECTING_HAND or G.CONTROLLER.locked then return end
        if not G.hand or #G.hand.cards < 5 then return end
        for _, card in ipairs(G.hand.cards) do if card.flipping then return end end
        _TUI_AUTOPLAY = false
        tap('left')
        expected = G.hand.cards[4]
        G.CONTROLLER:snap_to{node=expected}
        phase, started = 1, frame
    elseif phase == 1 and frame > started + 3 then
        assert(G.CONTROLLER.focused.target == expected, 'initial focus was lost')
        tap('left')
        tap('left')
        phase = 2
    elseif phase == 2 then
        for _ = 1, 2 do
            expected = G.hand.cards[expected.rank == 1 and #G.hand.cards or expected.rank - 1]
        end
        assert(G.CONTROLLER.focused.target == expected,
            'two left presses did not both move focus at move ' .. (moves + 1))
        moves = moves + 2
        if moves == 12 then
            tap('start')
            phase = 3
            return 'rapid-left'
        end
        tap('left')
        tap('left')
    elseif phase == 3 then
        assert(G.OVERLAY_MENU, 'Start did not open the menu')
        tap('b')
        started, phase = frame, 4
    elseif phase == 4 then
        if not G.OVERLAY_MENU then
            phase = 5
            io.stderr:write('[controls-test] 12 rapid left presses and immediate menu close passed\n')
            return 'rapid-finished'
        end
        assert(frame < started + 15, 'immediate menu close was lost during its opening lock')
    elseif phase == 5 then
        G.FUNCS.your_collection_spectrals()
        local change_page = G.FUNCS.your_collection_spectral_page
        G.FUNCS.your_collection_spectral_page = function(...)
            page_changes = page_changes + 1
            return change_page(...)
        end
        phase, started = 6, frame
    elseif phase == 6 and frame > started + 10 then
        tap('l1')
        tap('l1')
        phase = 7
    elseif phase == 7 then
        page_checks = page_checks + 1
        assert(page_changes == page_checks, 'rapid collection page change was lost')
        if page_checks % 2 == 0 then
            if page_checks == 6 then
                phase, started = 8, love.timer.getTime()
                return
            end
            tap('l1')
            tap('l1')
        end
    elseif phase == 8 and love.timer.getTime() - started > 1 then
        local count = 0
        for _, area in ipairs(G.your_collection) do
            for _, card in ipairs(area.cards) do
                count = count + 1
                assert((card.dissolve or 0) == 0, 'collection reveal remained unfinished')
            end
        end
        assert(count >= 7, 'collection cards disappeared')
        phase = 9
        io.stderr:write('[controls-test] PASS: rapid hand navigation, menu close and collection pages\n')
        return 'rapid-collection'
    end
end
