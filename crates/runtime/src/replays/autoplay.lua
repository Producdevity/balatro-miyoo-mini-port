local _auto_gp = {
    getGamepadMappingString = function() return 'balatro_tui' end,
    getGamepadAxis = function(self, axis) return 0 end,
    getName = function(self) return 'TUI Keyboard' end,
    isGamepadDown = function(self, btn) return false end,
}
local _auto_cooldown = 0
local _auto_last_state = nil
local _auto_round_eval_start = nil
local _auto_run_started = false
local _auto_blind_adjusted = false
local _auto_payout_jokers_added = false
local _auto_next_menu_frame = _TUI_AUTOPLAY_DELAY or 850
local function _auto_press(btn, frame)
    io.stderr:write(string.format("[AUTO] Press '%s' at F%d\n", btn, frame))
    love.gamepadpressed(_auto_gp, btn)
    _auto_cooldown = frame + 8
end
local function _auto_release(btn)
    love.gamepadreleased(_auto_gp, btn)
end
local _auto_pending_release = nil
return function(frame)
    if not _TUI_AUTOPLAY then return end
    if not G or not G.CONTROLLER then return end
    if frame % 120 == 0 then
        local function transform(object)
            local t = object and (object.VT or object.T)
            if not t then return '-' end
            return string.format('%.2f,%.2f %.2fx%.2f', t.x, t.y, t.w, t.h)
        end
        local blind_offset = G.HUD_blind and G.HUD_blind.alignment and
            G.HUD_blind.alignment.offset or nil
        io.stderr:write(string.format(
            '[AUTO-LAYOUT] room=%s hud=%s blind=%s blind-offset=%s hand=%s deck=%s jokers=%s\n',
            transform(G.ROOM), transform(G.HUD), transform(G.HUD_blind),
            blind_offset and string.format('%.2f,%.2f', blind_offset.x, blind_offset.y) or '-',
            transform(G.hand), transform(G.deck), transform(G.jokers)))
    end
    -- Release previous button
    if _auto_pending_release and frame >= _auto_pending_release[2] then
        _auto_release(_auto_pending_release[1])
        _auto_pending_release = nil
    end
    -- Cooldown between inputs
    if frame < _auto_cooldown then return end

    local state = G.STATE
    local stage = G.STAGE
    if not state or not G.STATES then return end

    -- SPLASH/MENU: press 'a' to start
    if stage == G.STAGES.MAIN_MENU then
        if frame >= _auto_next_menu_frame and not G.CONTROLLER.locked then
            if not _auto_run_started and G.FUNCS and G.FUNCS.start_run then
                local ok, err = pcall(function()
                    G.FUNCS.start_run(nil, {stake = 1, seed = _TUI_AUTOPLAY_SEED})
                end)
                if ok then
                    _auto_run_started = true
                    _auto_cooldown = frame + 120
                    io.stderr:write(string.format("[AUTO] start_run called at F%d\n", frame))
                    return
                end
                io.stderr:write(string.format("[AUTO] start_run error: %s\n", tostring(err)))
            end
            _auto_press('a', frame)
            _auto_pending_release = {'a', frame + 5}
            _auto_next_menu_frame = frame + (_TUI_AUTOPLAY_MENU_INTERVAL or 180)
        end
        return
    end

    -- State-aware gameplay automation
    if state == G.STATES.BLIND_SELECT then
        if _auto_last_state ~= 'BLIND_SELECT' then
            _auto_last_state = 'BLIND_SELECT'
            _auto_blind_adjusted = false
            _auto_cooldown = frame + (_TUI_AUTOPLAY_SETTLE_FRAMES or 240)
            io.stderr:write(string.format("[AUTO] Entered BLIND_SELECT at F%d, waiting for load lock\n", frame))
            return
        end
        -- Direct approach: call select_blind via the button system
        if not G.CONTROLLER.locked then
            io.stderr:write(string.format("[AUTO] Controller unlocked at F%d, selecting blind\n", frame))
            local ok, err = pcall(function()
                if G.FUNCS and G.FUNCS.select_blind then
                    local blind = string.lower(G.GAME.blind_on_deck or 'Small')
                    local option = G.blind_select_opts and G.blind_select_opts[blind]
                    local button = option and option:get_UIE_by_ID('select_blind_button')
                    if not button or not button.config.ref_table then
                        error(blind .. ' blind selection button is unavailable')
                    end
                    G.FUNCS.select_blind(button)
                end
            end)
            if ok then
                io.stderr:write(string.format("[AUTO] select_blind called successfully at F%d\n", frame))
                _auto_cooldown = frame + 120
            else
                io.stderr:write(string.format("[AUTO] select_blind error: %s\n", tostring(err)))
                -- Fallback: try dpdown + a
                _auto_press('a', frame)
                _auto_pending_release = {'a', frame + 5}
                _auto_cooldown = frame + 30
            end
        end
    elseif state == G.STATES.SELECTING_HAND then
        if _auto_last_state ~= 'SELECTING_HAND' then
            _auto_last_state = 'SELECTING_HAND'
            if not _auto_payout_jokers_added and _TUI_AUTOPLAY_PAYOUT_JOKERS > 0 then
                _auto_payout_jokers_added = true
                G.GAME.dollars = 100
                for i = 1, _TUI_AUTOPLAY_PAYOUT_JOKERS do
                    local card = create_card('Joker', G.jokers, nil, nil, true, nil, 'j_golden')
                    card:add_to_deck()
                    G.jokers:emplace(card)
                end
                io.stderr:write(string.format('[AUTO] Added %d payout Jokers\n',
                    _TUI_AUTOPLAY_PAYOUT_JOKERS))
            end
            if not _auto_blind_adjusted and _TUI_AUTOPLAY_TEST_BLIND_CHIPS and
                G.GAME and G.GAME.blind then
                G.GAME.blind.chips = _TUI_AUTOPLAY_TEST_BLIND_CHIPS
                G.GAME.blind.chip_text = tostring(_TUI_AUTOPLAY_TEST_BLIND_CHIPS)
                _auto_blind_adjusted = true
                io.stderr:write(string.format(
                    '[AUTO] Test blind target set to %d\n', _TUI_AUTOPLAY_TEST_BLIND_CHIPS))
            end
            if _TUI_AUTOPLAY_STRESS_EFFECTS and G.hand and G.hand.cards then
                local editions = {
                    {foil = true},
                    {holo = true},
                    {polychrome = true},
                }
                if _TUI_AUTOPLAY_NEGATIVE_CARDS then editions = {{negative = true}} end
                for i, card in ipairs(G.hand.cards) do
                    card:set_edition(editions[(i - 1) % #editions + 1], true, true)
                end
                io.stderr:write(string.format(
                    '[AUTO] Applied %s effects to %d cards\n',
                    _TUI_AUTOPLAY_NEGATIVE_CARDS and 'negative' or 'foil, holographic, and polychrome', #G.hand.cards))
            end
            _auto_cooldown = frame + math.max(30, math.floor((_TUI_AUTOPLAY_SETTLE_FRAMES or 240)/2))
            io.stderr:write(string.format("[AUTO] Entered SELECTING_HAND at F%d\n", frame))
            return
        end
        -- Highlight cards then play
        local ok, err = pcall(function()
            if not G.hand or not G.hand.cards or #G.hand.cards == 0 then
                return
            end
            -- Let the game's deal and flip animations finish before selecting cards.
            for i = 1, #G.hand.cards do
                if G.hand.cards[i].flipping then return end
            end
            -- Highlight first 5 cards
            if #G.hand.highlighted == 0 then
                local n = math.min(5, #G.hand.cards)
                for i = 1, n do
                    G.hand:add_to_highlighted(G.hand.cards[i], true)
                end
                io.stderr:write(string.format("[AUTO] Highlighted %d cards at F%d\n", n, frame))
                return  -- wait a frame before playing
            end
            if _TUI_AUTOPLAY_HOLD_HAND then return end
            -- Play the highlighted cards
            if G.FUNCS and G.FUNCS.play_cards_from_highlighted and #G.hand.highlighted > 0 then
                G.FUNCS.play_cards_from_highlighted({config = {id = 'play_button'}})
                io.stderr:write(string.format("[AUTO] Played %d cards at F%d\n", #G.hand.highlighted, frame))
            end
        end)
        if ok then
            _auto_cooldown = frame + 60
        else
            io.stderr:write(string.format("[AUTO] play error: %s\n", tostring(err)))
            _auto_cooldown = frame + 120
        end
    elseif state == G.STATES.SHOP then
        if _auto_last_state ~= 'SHOP' then
            _auto_last_state = 'SHOP'
            _auto_cooldown = frame + 180  -- wait 3 seconds for shop to load
            io.stderr:write(string.format("[AUTO] Entered SHOP at F%d\n", frame))
            return
        end
        if _TUI_AUTOPLAY_HOLD_SHOP then return end
        -- Try to leave shop and go to next round
        local ok, err = pcall(function()
            if G.FUNCS and G.FUNCS.toggle_shop then
                io.stderr:write(string.format("[AUTO] Calling toggle_shop at F%d\n", frame))
                G.FUNCS.toggle_shop({config = {id = 'next_round_button'}})
            else
                io.stderr:write(string.format("[AUTO] toggle_shop not found at F%d\n", frame))
            end
        end)
        if not ok then
            io.stderr:write(string.format("[AUTO] shop error: %s\n", tostring(err)))
        end
        _auto_cooldown = frame + 180
    elseif state == G.STATES.ROUND_EVAL then
        if _auto_last_state ~= 'ROUND_EVAL' then
            _auto_last_state = 'ROUND_EVAL'
            _auto_cooldown = frame + 60
            _auto_round_eval_start = frame
            _auto_cashed_out = false
            _auto_round_eval_ready = false
            G.CONTROLLER:set_gamepad(_auto_gp)
            G.CONTROLLER:set_HID_flags('button', 'a')
            io.stderr:write(string.format("[AUTO] Entered ROUND_EVAL at F%d\n", frame))
            return
        end
        -- Wait for events to drain to 0, then cash out
        local ev_count = 0
        if G.E_MANAGER and G.E_MANAGER.queues then
            for k,v in pairs(G.E_MANAGER.queues) do
                ev_count = ev_count + #v
            end
        end
        if ev_count == 0 and not _auto_cashed_out then
            if _TUI_AUTOPLAY_HOLD_ROUND_EVAL then
                if not _auto_round_eval_ready then
                    _auto_round_eval_ready = true
                    io.stderr:write(string.format("[AUTO] Payout ready at F%d\n", frame))
                end
                return
            end
            _auto_cashed_out = true
            local ok, err = pcall(function()
                local focused = G.CONTROLLER.focused.target
                if not focused or focused.config.id ~= 'cash_out_button' then
                    error('cash-out button is not focused')
                end
                _auto_press('a', frame)
                _auto_pending_release = {'a', frame + 5}
                io.stderr:write(string.format("[AUTO] cash-out input at F%d (waited %d frames)\n", frame, frame - _auto_round_eval_start))
            end)
            if not ok then
                io.stderr:write(string.format("[AUTO] cash_out error: %s\n", tostring(err)))
            end
            _auto_cooldown = frame + 60
        end
    elseif state == G.STATES.GAME_OVER then
        if _auto_last_state ~= 'GAME_OVER' then
            _auto_last_state = 'GAME_OVER'
            _auto_cooldown = frame + 180  -- wait 3s for game over screen to load
            io.stderr:write(string.format("[AUTO] GAME OVER at F%d\n", frame))
            return
        end
        -- Try to start a new run directly
        local ok, err = pcall(function()
            if G.FUNCS and G.FUNCS.go_to_menu then
                G.FUNCS.go_to_menu({config = {id = 'go_to_menu_button'}})
                io.stderr:write(string.format("[AUTO] go_to_menu at F%d\n", frame))
            end
        end)
        if not ok then
            io.stderr:write(string.format("[AUTO] game_over error: %s\n", tostring(err)))
            _auto_press('a', frame)
            _auto_pending_release = {'a', frame + 5}
        end
        _auto_cooldown = frame + 180
    elseif state == G.STATES.TAROT_PACK or state == G.STATES.PLANET_PACK
        or state == G.STATES.SPECTRAL_PACK or state == G.STATES.STANDARD_PACK
        or state == G.STATES.BUFFOON_PACK then
        -- Skip pack opening: just press 'b' to close
        if _auto_last_state ~= 'PACK' then
            _auto_last_state = 'PACK'
            _auto_cooldown = frame + 120
            io.stderr:write(string.format("[AUTO] Pack open at F%d, skipping\n", frame))
            return
        end
        -- Press 'b' (back) to skip pack
        _auto_press('b', frame)
        _auto_pending_release = {'b', frame + 5}
        _auto_cooldown = frame + 60
    else
        -- For transitional states, just log
        local sname = "?"
        if G.STATES then
            for k,v in pairs(G.STATES) do
                if v == state then sname = k; break end
            end
        end
        if _auto_last_state ~= sname then
            _auto_last_state = sname
            io.stderr:write(string.format("[AUTO] State=%s at F%d\n", sname, frame))
        end
    end
end
