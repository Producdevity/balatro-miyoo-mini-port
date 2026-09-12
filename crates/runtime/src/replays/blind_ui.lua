local phase, next_frame = 0, 0
local draw_frame, draw_count, draw_order = -1, 0, {}
local panel_after_cards, menu_after_panel
local score_contrast

local function track_draw(object, name)
    local original = object.draw
    object.draw = function(self, ...)
        if draw_frame ~= G.FRAMES.DRAW then
            draw_frame, draw_count, draw_order = G.FRAMES.DRAW, 0, {}
        end
        draw_count = draw_count + 1
        draw_order[name] = draw_count
        return original(self, ...)
    end
end

local function luminance(colour)
    local value = 0
    for i, weight in ipairs({0.2126, 0.7152, 0.0722}) do
        local c = colour[i]
        value = value + weight*(c <= 0.04045 and c/12.92 or ((c+0.055)/1.055)^2.4)
    end
    return value
end

return function(frame)
    assert(frame < 950 or phase == 8, 'blind UI test timed out')
    if phase == 0 then
        if G.STATE ~= G.STATES.BLIND_SELECT then return end
        _TUI_AUTOPLAY = false
        if not G.blind_select or G.CONTROLLER.locked then return end
        for _, key in ipairs({'j_joker', 'j_jolly', 'j_golden', 'j_mail'}) do
            local card = create_card('Joker', G.jokers, nil, nil, true, nil, key)
            card:add_to_deck()
            G.jokers:emplace(card)
        end
        track_draw(G.jokers, 'cards')
        track_draw(G.blind_select, 'panel')
        local button = G.blind_select_opts.small:get_UIE_by_ID('select_blind_button')
        G.CONTROLLER:snap_to{node=button}
        phase, next_frame = 1, frame + 90
    elseif phase == 1 and frame >= next_frame then
        assert(draw_order.cards and draw_order.panel, 'blind panel or Joker row was not drawn')
        for _, option in pairs(G.blind_select_opts) do
            local bounds = option.VT
            assert(bounds.y >= 0 and bounds.y+bounds.h <= G.TILE_H-0.1,
                string.format('blind choice outside screen: y=%.3f h=%.3f root_y=%.3f root_h=%.3f',
                    bounds.y, bounds.h, G.blind_select.T.y, G.blind_select.T.h))
        end
        panel_after_cards = draw_order.panel > draw_order.cards
        phase, next_frame = 2, frame + 1
        return 'blind-ui-selection'
    elseif phase == 2 and frame >= next_frame then
        G.FUNCS.options()
        track_draw(G.OVERLAY_MENU, 'menu')
        phase, next_frame = 3, frame + 45
    elseif phase == 3 and frame >= next_frame then
        assert(draw_order.menu, 'pause menu was not drawn')
        menu_after_panel = not draw_order.panel or draw_order.menu > draw_order.panel
        phase, next_frame = 4, frame + 1
        return 'blind-ui-pause'
    elseif phase == 4 and frame >= next_frame then
        G.FUNCS.exit_overlay_menu()
        phase, next_frame = 5, frame + 30
    elseif phase == 5 and frame >= next_frame and not G.CONTROLLER.locked then
        G.GAME.round_resets.ante = 3
        G.GAME.blind_on_deck = 'Boss'
        G.FUNCS.select_blind({config={ref_table=G.P_BLINDS.bl_flint}})
        phase = 6
    elseif phase == 6 and G.STATE == G.STATES.SELECTING_HAND and not G.CONTROLLER.locked then
        G.GAME.chips = 1710
        G.GAME.blind.chips = 4000
        G.GAME.blind.chip_text = number_format(4000)
        phase, next_frame = 7, frame + 90
    elseif phase == 7 and frame >= next_frame then
        local target = G.HUD:get_UIE_by_ID('small_screen_blind_target')
        assert(target.small_screen_target_text == '/ 4,000', 'target score was not updated')
        local background = target.parent.config.colour
        if background[4] < 1 then background = G.C.DYN_UI.BOSS_MAIN end
        score_contrast = (luminance(G.C.RED)+0.05)/(luminance(background)+0.05)
        io.stderr:write(string.format('[blind-ui] target contrast=%.2f panel_after_cards=%s menu_after_panel=%s\n',
            score_contrast, tostring(panel_after_cards), tostring(menu_after_panel)))
        phase, next_frame = 8, frame + 1
        return 'blind-ui-score'
    elseif phase == 8 and frame == next_frame then
        assert(score_contrast >= 4.5, 'target score does not have enough contrast')
        assert(panel_after_cards, 'Jokers were drawn over the blind-selection panel')
        assert(menu_after_panel, 'blind-selection panel was drawn over the pause menu')
        io.stderr:write('[controls-test] PASS: blind score contrast and panel draw order\n')
    end
end
