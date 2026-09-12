local languages, index, changed_at, previous_menu, started_at, selector_at, settled_at
local play = os.getenv('BALATRO_TEST_CONTROLS') == 'languages-play'
local gameplay_at, captured_hand, shop_at, shop_card

return function(frame)
    if frame == 1 then _TUI_AUTOPLAY = false end
    if gameplay_at then
        assert(love.timer.getTime() - gameplay_at < 60, 'translated round did not reach the shop')
        assert(G.LANG.key == 'zh_CN', 'gameplay changed the selected language')
        if G.STATE == G.STATES.SELECTING_HAND and G.hand and #G.hand.cards > 0 and
            not G.CONTROLLER.locked and not captured_hand then
            captured_hand = true
            return 'language-gameplay-zh_CN'
        end
        if G.STATE == G.STATES.SHOP and G.shop_jokers and
            #G.shop_jokers.cards > 0 and not G.CONTROLLER.locked then
            if not shop_at then
                shop_card = G.shop_jokers.cards[1]
                G.CONTROLLER:snap_to{node=shop_card}
                shop_at = frame + 45
                return
            end
            if frame < shop_at then return end
            assert(captured_hand, 'translated round never dealt cards')
            assert(shop_card.children.h_popup, 'translated shop tooltip did not appear')
            io.stderr:write('[controls-test] PASS: translated round and shop\n')
            gameplay_at = nil
            _TUI_AUTOPLAY = false
            return 'language-shop-zh_CN'
        end
        return
    end
    if frame == 60 then
        G:main_menu()
        languages = {}
        local last = play and 'zh_CN' or 'en-us'
        for key, language in pairs(G.LANGUAGES) do
            if not language.omit and key ~= last then
                languages[#languages+1] = key
            end
        end
        table.sort(languages)
        languages[#languages+1] = last
        index = 0
    end
    if not languages or frame < 100 then return end
    if changed_at then
        assert(love.timer.getTime() - started_at < 15, 'language change did not finish')
        if frame - changed_at < 60 or G.CONTROLLER.locked or G.screenwipe then return end
        if not G.MAIN_MENU_UI or G.MAIN_MENU_UI == previous_menu then return end
        settled_at = settled_at or love.timer.getTime()
        if love.timer.getTime() - settled_at < 2 then return end
        assert(G.LANG.key == languages[index], 'wrong language selected')
        assert(G.LANG.font.FONT:getWidth(localize('b_play_cap')) > 0,
            'translated menu has no font metrics')
        io.stderr:write('[language-test] menu rebuilt: '..languages[index]..'\n')
        changed_at = nil
        settled_at = nil
        if index == #languages then
            io.stderr:write('[controls-test] PASS: all language menu transitions\n')
            languages = nil
            if play then
                assert(_TUI_AUTOPLAY_SEED, 'languages-play requires TUI_AUTOPLAY=1')
                gameplay_at = love.timer.getTime()
                _TUI_AUTOPLAY_HOLD_SHOP = true
                _TUI_AUTOPLAY = true
            end
        end
        return 'language-'..G.LANG.key
    end
    if not G.MAIN_MENU_UI or G.CONTROLLER.locked or G.screenwipe then return end
    if not selector_at then
        io.stderr:write('[language-test] opening selector\n')
        G.FUNCS.language_selection()
        selector_at = frame
        return
    end
    if frame == selector_at + 20 then return 'language-selector-'..G.LANG.key end
    if frame <= selector_at + 20 then return end
    selector_at = nil
    index = index + 1
    local key = languages[index]
    previous_menu = G.MAIN_MENU_UI
    io.stderr:write('[language-test] selecting '..key..' from '..G.LANG.key..'\n')
    G.FUNCS.change_lang({config = {ref_table = G.LANGUAGES[key]}})
    changed_at = frame
    started_at = love.timer.getTime()
end
