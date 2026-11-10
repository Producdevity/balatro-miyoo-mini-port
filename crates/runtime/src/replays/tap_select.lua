-- Quick D-pad taps followed by A must select the card two steps away,
-- whether the taps arrive in one update or are split across updates.
-- BALATRO_TAP_EVDEV=/dev/input/event0 injects raw key records on the device,
-- so the taps also pass through the kernel and the evdev reader.
local evdev = os.getenv('BALATRO_TAP_EVDEV')
local codes = {left=105, right=106, a=57}
local function record(type, code, value)
    local function u16(v) return string.char(v % 256, math.floor(v / 256) % 256) end
    return string.rep('\0', 8) .. u16(type) .. u16(code) .. u16(value) .. u16(0)
end
local function tap(key)
    if evdev then
        local file = assert(io.open(evdev, 'wb'))
        file:write(record(1, codes[key], 1) .. record(0, 0, 0) ..
            record(1, codes[key], 0) .. record(0, 0, 0))
        file:close()
        return
    end
    love.keypressed('miyoo_' .. key, 'miyoo_' .. key, false)
    love.keyreleased('miyoo_' .. key, 'miyoo_' .. key)
end

-- Each case: taps per update, and the rank offsets from the start card that
-- must end up highlighted.
local cases = {
    {{{'left', 'left', 'a'}}, {-2}},
    {{{'left'}, {'left', 'a'}}, {-2}},
    {{{'left', 'left'}, {'a'}}, {-2}},
    {{{'right', 'right', 'a'}}, {2}},
    {{{'right'}, {'right', 'a'}}, {2}},
    {{{'a', 'left', 'left', 'a'}}, {0, -2}},
    {{{'a', 'right', 'right', 'a', 'left', 'a'}}, {0, 1, 2}},
}
local case, step, phase, start_card, target, wait = 1, 1, 0, nil, nil, 0
return function(frame)
    assert(frame < 2500 or phase == 4, 'tap-select test timed out in case ' .. case)
    if phase == 0 then
        if G.STATE ~= G.STATES.SELECTING_HAND or G.CONTROLLER.locked then return end
        if not G.hand or #G.hand.cards < 7 then return end
        for _, card in ipairs(G.hand.cards) do if card.flipping then return end end
        _TUI_AUTOPLAY = false
        G.hand:unhighlight_all()
        -- Switch the game to controller focus without going through evdev.
        love.keypressed('miyoo_left', 'miyoo_left', false)
        love.keyreleased('miyoo_left', 'miyoo_left')
        start_card = G.hand.cards[4]
        G.CONTROLLER:snap_to{node=start_card}
        phase, wait = 1, frame + 4
    elseif phase == 1 and frame > wait then
        assert(G.CONTROLLER.focused.target == start_card, 'initial focus was lost')
        target = {}
        for _, offset in ipairs(cases[case][2]) do
            target[G.hand.cards[start_card.rank + offset]] = true
        end
        phase, step = 2, 1
    elseif phase == 2 then
        for _, key in ipairs(cases[case][1][step]) do tap(key) end
        step = step + 1
        if step > #cases[case][1] then phase, wait = 3, frame + 40 end
    elseif phase == 3 and frame > wait then
        local got = {}
        for _, card in ipairs(G.hand.highlighted) do got[#got + 1] = card.rank - start_card.rank end
        local ok = #G.hand.highlighted == #cases[case][2]
        for _, card in ipairs(G.hand.highlighted) do ok = ok and target[card] end
        assert(ok, string.format('case %d highlighted offsets {%s}, expected {%s}',
            case, table.concat(got, ','), table.concat(cases[case][2], ',')))
        io.stderr:write(string.format('[controls-test] tap-select case %d ok\n', case))
        case = case + 1
        if case > #cases then
            io.stderr:write('[controls-test] PASS: quick taps then A select the right card\n')
            phase = 4
            return 'tap-select'
        end
        phase = 0
    end
end
