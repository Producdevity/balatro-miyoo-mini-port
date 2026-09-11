local selecting_at, scoring_at, checked_hand, checked_play

local function describe_box(label, box)
    if not box then return end
    for _, key in ipairs({'T', 'VT'}) do
        local t = box[key]
        io.stderr:write(string.format('[layout] %s.%s %.3f %.3f %.3f %.3f\n',
            label, key, t.x, t.y, t.w, t.h))
    end
end

local function check_hand()
    local expected_bottom = G.TILE_H - 2.35
    assert(math.abs(G.hand.T.y + G.hand.T.h - expected_bottom) < 0.05,
        'hand applies the stock bottom inset in addition to the compact layout')
    for _, card in ipairs(G.hand.cards) do
        assert(card.VT.y + card.VT.h < G.TILE_H - 1.8,
            'hand card covers the bottom actions')
    end
    checked_hand = true
    describe_box('hud', G.HUD)
    describe_box('hud-root', G.HUD.UIRoot)
    local focus = G.CONTROLLER.focused.target
    if focus then
        describe_box('focus', focus)
        local popup = focus.children.h_popup
        describe_box('popup', popup)
        if popup then describe_box('popup-root', popup.UIRoot) end
    end
    io.stderr:write('[controls-test] PASS: hand and bottom action spacing\n')
end

local function check_play()
    local bottom = 0
    for _, area in ipairs({G.jokers, G.consumeables}) do
        for _, card in ipairs(area.cards) do
            bottom = math.max(bottom, card.VT.y + card.VT.h)
        end
    end
    assert(bottom > 0, 'scoring layout test needs owned Jokers')
    local highest = G.play.T.y + (G.play.T.h - G.CARD_H)/2 - G.HIGHLIGHT_H
    assert(highest > bottom, 'highlighted scoring cards overlap the owned-card row')
    for _, card in ipairs(G.play.cards) do
        assert(card.VT.y > bottom, 'played card overlaps an owned card')
    end
    checked_play = true
    io.stderr:write('[controls-test] PASS: scoring cards remain below owned cards\n')
end

return function(frame)
    if G.STATE == G.STATES.SELECTING_HAND and not checked_hand then
        selecting_at = selecting_at or frame + 45
        if frame == selecting_at then
            check_hand()
            return 'hand-spacing'
        end
    end
    if G.STATE == G.STATES.HAND_PLAYED and #G.play.cards > 0 and not checked_play then
        scoring_at = scoring_at or frame + 20
        if frame >= scoring_at then
            check_play()
            return 'scoring-spacing'
        end
    elseif not checked_play then
        scoring_at = nil
    end
    if checked_play and frame == scoring_at + 60 then return 'scoring-settled' end
end
