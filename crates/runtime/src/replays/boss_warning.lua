-- The Psychic's "hand will not score" warning gets a readable dark plate.
local phase, wait = 0, 0
return function(frame)
    assert(frame < 3000 or phase == 2, 'boss-warning test timed out in phase ' .. phase)
    if phase == 0 then
        if G.STATE ~= G.STATES.SELECTING_HAND or G.CONTROLLER.locked then return end
        if not G.hand or #G.hand.cards < 2 then return end
        for _, card in ipairs(G.hand.cards) do if card.flipping then return end end
        _TUI_AUTOPLAY = false
        G.GAME.blind:set_blind(G.P_BLINDS.bl_psychic)
        G.hand:add_to_highlighted(G.hand.cards[1])
        G.hand:add_to_highlighted(G.hand.cards[2])
        phase, wait = 1, frame + 60
    elseif phase == 1 and frame > wait then
        local warning = G.boss_warning_text
        assert(warning, 'no boss warning shown')
        local plate = warning.UIRoot.config.colour
        assert(plate[4] > 0.5, 'boss warning has no backing plate')
        assert(plate[1] + plate[2] + plate[3] < 0.6, 'boss warning plate is not dark')
        io.stderr:write('[controls-test] PASS: boss warning has a dark plate\n')
        phase = 2
        return 'boss-warning'
    end
end
