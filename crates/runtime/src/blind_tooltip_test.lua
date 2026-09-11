local phase, next_frame = 0, 0
local button, select_button, popup

local function find_button(node, name)
    if node.config and node.config.button == name then return node end
    for _, child in pairs(node.children or {}) do
        local found = find_button(child, name)
        if found then return found end
    end
end

local function focus(node)
    love.keypressed('miyoo_down', 'miyoo_down', false)
    love.keyreleased('miyoo_down', 'miyoo_down')
    G.CONTROLLER:snap_to{node=node}
end

return function(frame)
    assert(frame < 1200 or phase == 5, 'blind tooltip test timed out')
    if phase == 0 then
        if not G or G.STATE ~= G.STATES.BLIND_SELECT then return end
        _TUI_AUTOPLAY = false
        local option = G.blind_select_opts and G.blind_select_opts.small
        if G.CONTROLLER.locked or not option then return end
        button = find_button(option.UIRoot, 'skip_blind')
        select_button = option:get_UIE_by_ID('select_blind_button')
        if not button or not select_button then return end
        focus(button)
        phase, next_frame = 1, frame + 60
    elseif phase == 1 and frame >= next_frame then
        assert(G.CONTROLLER.focused.target == button, 'Skip Blind lost controller focus')
        popup = button.parent.children.alert
        assert(popup, 'Skip Blind tooltip is missing')
        phase, next_frame = 2, frame + 180
        return 'skip-blind-open'
    elseif phase == 2 or phase == 4 then
        assert(G.CONTROLLER.focused.target == button, 'idle Skip Blind focus changed')
        assert(button.parent.children.alert == popup, 'Skip Blind tooltip was removed or recreated')
        assert(popup.states.visible, 'Skip Blind tooltip became hidden')
        if frame >= next_frame then
            if phase == 2 then
                focus(select_button)
                phase, next_frame = 3, frame + 60
                return 'skip-blind-held'
            end
            phase = 5
            io.stderr:write('[controls-test] PASS: Skip Blind tooltip stays open, dismisses, and reopens\n')
            return 'skip-blind-reopened'
        end
        if frame % 30 == 0 then return 'skip-blind-' .. frame end
    elseif phase == 3 and frame >= next_frame then
        assert(G.CONTROLLER.focused.target == select_button, 'Select Blind focus was lost')
        assert(not button.parent.children.alert, 'Skip Blind tooltip remained after leaving')
        focus(button)
        phase, next_frame = 6, frame + 60
        return 'skip-blind-closed'
    elseif phase == 6 and frame >= next_frame then
        popup = button.parent.children.alert
        assert(popup, 'Skip Blind tooltip did not reopen')
        phase, next_frame = 4, frame + 180
    end
end
