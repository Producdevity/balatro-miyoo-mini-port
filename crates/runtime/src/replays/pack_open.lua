-- Opening a shop booster must move the shop panel off screen.
local phase, wait = 0, 0
return function(frame)
    assert(frame < 4000 or phase == 2, 'pack-open test timed out in phase ' .. phase)
    if phase == 0 then
        if G.STATE ~= G.STATES.SHOP or G.CONTROLLER.locked or not G.shop_booster then return end
        if #G.shop_booster.cards == 0 then return end
        _TUI_AUTOPLAY = false
        G.GAME.dollars = 100
        G.FUNCS.use_card({config={ref_table=G.shop_booster.cards[1]}})
        phase, wait = 1, frame + 90
    elseif phase == 1 and frame > wait then
        assert(G.STATE ~= G.STATES.SHOP, 'pack did not open')
        assert(G.shop.VT.y > G.ROOM.T.h, 'shop panel stayed on screen behind the pack')
        io.stderr:write('[controls-test] PASS: shop hidden while a pack is open\n')
        phase = 2
        return 'pack-open'
    end
end
