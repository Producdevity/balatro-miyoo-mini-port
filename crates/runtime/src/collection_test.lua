local started, start_frame, next_page, cycle, captured
local slow = os.getenv('BALATRO_TEST_CONTROLS') == 'collection-slow'
local page_button

local function find_page_button(node)
    local config = node.config
    if config and config.button == 'option_cycle' and config.ref_value == 'r' and
        config.ref_table.opt_callback == 'your_collection_spectral_page' then
        return node
    end
    for _, child in pairs(node.children or {}) do
        local found = find_page_button(child)
        if found then return found end
    end
end

local function begin(frame)
    cycle = (cycle or 0) + 1
    if cycle == 1 then
        G.FUNCS.your_collection_spectrals()
        page_button = assert(find_page_button(G.OVERLAY_MENU.UIRoot),
            'collection page button is missing')
    else
        G.FUNCS.option_cycle(page_button)
    end
    started, start_frame, captured = love.timer.getTime(), frame, false
end

return function(frame)
    if frame == 60 then
        _TUI_AUTOPLAY = false
        G:main_menu()
    end
    if frame == 100 then begin(frame) end
    if next_page == frame then
        next_page = nil
        begin(frame)
    end
    if not started then return end
    local elapsed = love.timer.getTime() - started
    local count, revealing = 0, false
    for _, area in ipairs(G.your_collection) do
        for _, card in ipairs(area.cards) do
            count = count + 1
            revealing = revealing or (card.dissolve or 0) > 0
        end
    end
    assert(count >= 7, 'collection page did not create its cards')
    assert(elapsed < 15, 'collection reveal did not finish')
    if not revealing and frame > start_frame then
        local frames = frame - start_frame
        assert(not slow or elapsed < 1.2, 'menu reveal follows the capped game clock')
        io.stderr:write(string.format(
            '[collection-test] page=%d cards=%d reveal_seconds=%.3f frames=%d fps=%.2f\n',
            cycle, count, elapsed, frames, frames/elapsed))
        started = nil
        if cycle < 6 then next_page = frame + (slow and 4 or 30)
        else io.stderr:write('[controls-test] PASS: six spectral collection reveals\n') end
        return 'collection-page-'..cycle
    end
    if elapsed > 0.25 and not captured then
        captured = true
        return 'collection-reveal-'..cycle
    end
end
