local previous_state, previous_round, sample_at, rounds = nil, nil, nil, 0
local frame_limit = assert(tonumber(os.getenv('BALATRO_TEST_FRAMES')))
local minimum_rounds = tonumber(os.getenv('BALATRO_TEST_MIN_ROUNDS')) or 6

local function sample(frame)
    if previous_round == G.GAME.round then return end
    previous_round = G.GAME.round
    rounds = rounds + 1
    local live, classified = {}, {}
    for _, object in ipairs(G.MOVEABLES) do
        local kind = 'Moveable'
        for _, name in ipairs({'UIBox', 'UIElement', 'Card', 'Sprite', 'Particles', 'DynaText'}) do
            if getmetatable(object) == _G[name] then kind = name end
        end
        assert(not live[object], 'duplicate '..kind..' in movement list')
        assert(not object.REMOVED, 'removed object remains in movement list')
        if object.role and object.role.major and object.role.major.REMOVED then
            error(kind..' still attached to a removed major')
        end
        live[object] = true
    end
    for _, list in ipairs({G.SVMM_MAJORS, G.SVMM_MINORS, G.SVMM_GLUED}) do
        for _, object in ipairs(list) do
            assert(live[object], 'removed object remains in movement list')
            assert(not classified[object], 'object appears in multiple movement lists')
            classified[object] = true
        end
    end
    for _, object in ipairs(G.SVMM_UPDATEABLES) do
        assert(live[object], 'removed object remains in update list')
    end
    for _, box in ipairs(G.I.UIBOX) do
        assert(not box:get_UIE_by_ID('cash_out_button'),
            'Cash Out box survived into the next blind')
    end
    io.stderr:write(string.format(
        '[long-run] hand=%d frame=%d ante=%d round=%d lua_kib=%.0f moveables=%d uiboxes=%d cards=%d\n',
        rounds, frame, G.GAME.round_resets.ante, G.GAME.round,
        collectgarbage('count'), #G.MOVEABLES, #G.I.UIBOX, #G.I.CARD))
end

return function(frame)
    if G.STATE ~= previous_state then
        previous_state = G.STATE
        sample_at = G.STATE == G.STATES.SELECTING_HAND and frame + 30 or nil
    end
    if sample_at and frame >= sample_at then
        sample_at = nil
        sample(frame)
    end
    if frame == frame_limit then
        assert(rounds >= minimum_rounds, 'long-run test did not complete enough rounds')
        io.stderr:write(string.format('[controls-test] PASS: %d rounds without stale payout controls\n', rounds))
    end
end
