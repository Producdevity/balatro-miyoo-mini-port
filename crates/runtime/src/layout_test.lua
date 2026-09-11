local menu_at, shop_at, card, tooltip
local function overlap(a, b)
    return math.max(0, math.min(a.x+a.w,b.x+b.w)-math.max(a.x,b.x))*
           math.max(0, math.min(a.y+a.h,b.y+b.h)-math.max(a.y,b.y))
end

local function check_text(node)
    local object = node.config and node.config.object
    if object and getmetatable(object) == DynaText then
        for _, text in ipairs(object.strings) do
            for _, letter in ipairs(text.letters) do
                local expected = object.font.FONT:getWidth(letter.char)*object.scale +
                    2.7*(object.config.spacing or 0)*object.layout_font_scale/object.font.FONTSCALE
                assert(math.abs(letter.dims.x-expected) < 0.01,
                    'popup letter spacing does not match its scale')
            end
        end
    end
    for _, child in pairs(node.children or {}) do check_text(child) end
end

local function check_menu()
    local buttons = {}
    local function collect(node)
        if node.config and node.config.button then
            local t = node.VT
            assert(t.x >= 0 and t.y >= 0 and t.x+t.w <= G.TILE_W and
                t.y+t.h <= G.TILE_H, 'main menu button exceeds the screen')
            for _, other in ipairs(buttons) do
                assert(overlap(t, other) < 0.01, 'main menu buttons overlap')
            end
            buttons[#buttons+1] = t
            return
        end
        for _, child in pairs(node.children or {}) do collect(child) end
    end
    collect(G.MAIN_MENU_UI.UIRoot)
    assert(#buttons >= 5, 'main menu actions or language selector are missing')
    io.stderr:write('[controls-test] PASS: main menu button bounds\n')
end

return function(frame)
    if frame == 60 then G:main_menu() end
    if G.MAIN_MENU_UI and G.MAIN_MENU_UI.states.visible and not menu_at then
        menu_at = frame + 30
    end
    if frame == menu_at then
        check_menu()
        return 'main-menu'
    end
    if frame == 30 then return 'splash' end
    if G.STATE ~= G.STATES.SHOP or not G.shop_jokers or G.CONTROLLER.locked then return end
    if not shop_at then
        _TUI_AUTOPLAY = false
        card = G.shop_jokers.cards[#G.shop_jokers.cards]
        if not card then return end
        card.config.center.discovered = false
        card.bypass_discovery_center = false
        card.bypass_discovery_ui = false
        card.bypass_lock = false
        G.CONTROLLER:snap_to{node=card}
        shop_at = frame + 45
        return
    end
    if frame < shop_at then return end
    if frame == shop_at then
        tooltip = card.children.h_popup
        assert(tooltip, 'shop card has no tooltip')
        assert(overlap(tooltip.VT, card.VT) == 0, 'shop tooltip covers the focused card')
        check_text(tooltip.UIRoot)
        io.stderr:write('[controls-test] PASS: shop tooltip placement and animated text metrics\n')
        return 'shop-tooltip'
    end
    if frame < shop_at + 90 then
        assert(card.children.h_popup == tooltip, 'shop tooltip was recreated while focused')
    end
end
