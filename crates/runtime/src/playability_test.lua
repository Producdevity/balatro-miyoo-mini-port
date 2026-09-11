local function tap(key)
    love.keypressed('miyoo_' .. key, 'miyoo_' .. key, false)
    love.keyreleased('miyoo_' .. key, 'miyoo_' .. key)
end

local index, ready_at, deadline = 1, 0, 0
local first_card, selected_card, discards, hands
local watched_card, watched_popup, watch_until
local retired_tabs = setmetatable({}, {__mode = 'v'})
local retired_count = 0
local function next_deck_tab()
    retired_count = retired_count + 1
    retired_tabs[retired_count] = G.OVERLAY_MENU:get_UIE_by_ID('tab_contents').config.object
    tap('r1')
end
local function assert_in_room(node, label)
    local t = node.VT or node.T
    assert(t.x >= -0.1 and t.y >= -0.1 and
        t.x + t.w <= G.TILE_W + 0.1 and t.y + t.h <= G.TILE_H + 0.1,
        string.format('%s exceeds screen: %.2f %.2f %.2f %.2f', label, t.x, t.y, t.w, t.h))
end

local function assert_deck_fits()
    -- The outer root is the game's oversized dimming background, not the menu panel.
    assert_in_room(G.OVERLAY_MENU.UIRoot.children[1], 'deck panel')
end

local steps = {
    function()
        if G.STATE ~= G.STATES.SELECTING_HAND or G.CONTROLLER.locked then return false end
        if not G.hand or #G.hand.cards < 3 then return false end
        for _, card in ipairs(G.hand.cards) do if card.flipping then return false end end
        _TUI_AUTOPLAY = false
        G.hand:unhighlight_all()
        first_card = G.hand.cards[1]
        tap('left')
        G.CONTROLLER:snap_to{node = first_card}
        return true, 'initial'
    end,
    function()
        assert(G.CONTROLLER.focused.target == first_card, 'initial card focus was lost')
        for i, card in ipairs(G.hand.cards) do
            assert(card.rank == i, 'card order and controller ranks disagree')
        end
        selected_card = G.hand.cards[first_card.rank % #G.hand.cards + 1]
        assert(selected_card ~= first_card, 'navigation target did not advance')
        io.stderr:write(string.format('[controls-test] before right: first=%s rank=%s next=%s rank=%s locked=%s frame-lock=%s\n',
            tostring(first_card.ID), tostring(first_card.rank), tostring(selected_card.ID),
            tostring(selected_card.rank), tostring(G.CONTROLLER.locked), tostring(G.CONTROLLER.locks.frame)))
        tap('right')
        tap('a')
        return true, 'focused'
    end,
    function()
        assert(G.CONTROLLER.focused.target == selected_card, 'D-pad did not focus the next card')
        assert(#G.hand.highlighted == 1 and G.hand.highlighted[1] == selected_card,
            'A did not select the focused card after navigation')
        tap('a')
        tap('a')
        return true, 'selected'
    end,
    function()
        assert(#G.hand.highlighted == 1 and G.hand.highlighted[1] == selected_card,
            'two A taps did not deselect and reselect the same card')
        tap('b')
        return true
    end,
    function()
        assert(#G.hand.highlighted == 0, 'B did not clear selection')
        discards = G.GAME.current_round.discards_left
        tap('a')
        tap('y')
        return true
    end,
    function()
        if G.STATE ~= G.STATES.SELECTING_HAND or G.CONTROLLER.locked then return false end
        if G.GAME.current_round.discards_left ~= discards - 1 then return false end
        hands = G.GAME.current_round.hands_left
        tap('a')
        tap('x')
        return true, 'discarded'
    end,
    function()
        if G.STATE ~= G.STATES.SELECTING_HAND or G.CONTROLLER.locked then return false end
        if G.GAME.current_round.hands_left ~= hands - 1 then return false end
        tap('start')
        return true, 'played'
    end,
    function()
        assert(G.OVERLAY_MENU, 'Start did not open the menu')
        tap('b')
        return true, 'menu'
    end,
    function()
        assert(not G.OVERLAY_MENU, 'B did not close the menu')
        love.keypressed('miyoo_l2', 'miyoo_l2', false)
        return true, 'finished'
    end,
    function()
        assert(G.deck_preview, 'held L2 did not open the deck preview')
        assert_in_room(G.deck_preview, 'deck preview')
        assert(G.deck_preview.config.instance_type == 'POPUP', 'deck preview uses the background UI layer')
        local registered = false
        for _, box in ipairs(G.I.POPUP) do registered = registered or box == G.deck_preview end
        assert(registered, 'deck preview is not in the foreground popup pass')
        for _, box in ipairs(G.I.UIBOX) do assert(box ~= G.deck_preview, 'deck preview is drawn twice') end
        local t = G.deck_preview.T
        io.stderr:write(string.format('[controls-test] deck preview: %.2f %.2f %.2f %.2f room %.2f %.2f\n',
            t.x,t.y,t.w,t.h,G.ROOM.T.w,G.ROOM.T.h))
        love.keyreleased('miyoo_l2', 'miyoo_l2')
        return true, 'deck-preview'
    end,
    function()
        assert(not G.deck_preview, 'deck preview remained after releasing L2')
        tap('r2')
        return true
    end,
    function()
        assert(G.OVERLAY_MENU, 'R2 did not open the deck overview')
        assert_deck_fits()
        local t = G.OVERLAY_MENU.T
        io.stderr:write(string.format('[controls-test] deck overview: %.2f %.2f %.2f %.2f room %.2f %.2f\n',
            t.x,t.y,t.w,t.h,G.ROOM.T.w,G.ROOM.T.h))
        next_deck_tab()
        return true, 'deck-overview'
    end,
    function()
        assert(G.OVERLAY_MENU, 'R1 closed the deck overview')
        assert_deck_fits()
        tap('b')
        return true, 'deck-tab'
    end,
    function()
        assert(not G.OVERLAY_MENU, 'B did not close the deck overview')
        watched_card = G.hand.cards[2]
        G.CONTROLLER:snap_to{node = watched_card}
        return true
    end,
    function()
        assert(G.CONTROLLER.focused.target == watched_card, 'card focus was lost after closing deck')
        watched_popup = watched_card.children.h_popup
        assert(watched_popup, 'focused card has no tooltip')
        local a, b = watched_popup.VT, watched_card.VT
        assert(a.x+a.w <= b.x or b.x+b.w <= a.x or a.y+a.h <= b.y or b.y+b.h <= a.y,
            'tooltip covers the focused card')
        watch_until = true
        return true
    end,
}

for pass = 1, 12 do
    table.insert(steps, 13, function()
        assert(G.OVERLAY_MENU, 'deck tab switch closed the menu')
        assert_deck_fits()
        next_deck_tab()
        return true, 'deck-repeat-' .. pass
    end)
end

return function(frame)
    if watch_until then
        if watch_until == true then watch_until = frame + 90 end
        assert(G.CONTROLLER.focused.target == watched_card, 'idle card focus changed')
        assert(watched_card.children.h_popup == watched_popup, 'idle tooltip was removed or recreated')
        assert(watched_popup.states.visible, 'idle tooltip became hidden')
        if frame == watch_until then
            watch_until = nil
            assert(next(retired_tabs) == nil, 'removed deck tabs were not collected')
            io.stderr:write('[controls-test] PASS: navigation, selection, repeated taps, discard, play, menu, deck tabs, stable tooltip\n')
        end
        if frame % 15 == 0 then return 'tooltip-' .. frame end
        return
    end
    if index > #steps or frame < ready_at then return end
    if deadline == 0 then deadline = frame + 600 end
    assert(frame <= deadline, 'controller test timed out at step ' .. index)
    local done, capture = steps[index]()
    if done then
        io.stderr:write(string.format('[controls-test] step %d at frame %d lua=%.1fKiB\n', index, frame, collectgarbage('count')))
        index = index + 1
        ready_at, deadline = frame + 30, frame + 630
    end
    return capture
end
