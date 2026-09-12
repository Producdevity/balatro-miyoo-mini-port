local page, changed_at, seen, next_page, cycle_button

local function find_cycle(node)
    local config = node.config or {}
    if config.button == 'option_cycle' and config.ref_value == 'r' and config.ref_table and
        config.ref_table.opt_callback == 'your_collection_joker_page' then return node end
    for _, child in pairs(node.children or {}) do
        local found = find_cycle(child)
        if found then return found end
    end
end

return function(frame)
    if frame == 1 then _TUI_AUTOPLAY = false end
    if frame == 60 then G:main_menu() end
    if frame == 180 then
        seen = {}
        for _, center in ipairs(G.P_CENTER_POOLS.Joker) do
            center.unlocked = true
            center.discovered = true
        end
        G.FUNCS.your_collection_jokers()
        cycle_button = assert(find_cycle(G.OVERLAY_MENU.UIRoot), 'joker page control not found')
        page, changed_at = 1, frame
    end
    if next_page then
        G.FUNCS.option_cycle(cycle_button)
        page, changed_at, next_page = next_page, frame, nil
        return
    end
    if not page or frame - changed_at < 90 then return end
    for _, area in ipairs(G.your_collection) do
        for _, card in ipairs(area.cards) do
            local center = card.config.center
            local sprite = card.children.center
            local atlas = assert(sprite.atlas, center.key..': missing atlas')
            assert(atlas.image:getWidth() >= (center.pos.x + 1)*atlas.px,
                center.key..': sprite outside atlas width')
            assert(atlas.image:getHeight() >= (center.pos.y + 1)*atlas.py,
                center.key..': sprite outside atlas height')
            assert(not seen[center.key], center.key..': duplicate collection card')
            seen[center.key] = true
        end
    end
    local label = 'jokers-page-'..page
    if page == math.ceil(#G.P_CENTER_POOLS.Joker/15) then
        local count = 0
        for _ in pairs(seen) do count = count + 1 end
        assert(count == #G.P_CENTER_POOLS.Joker, 'collection omitted jokers')
        io.stderr:write('[controls-test] PASS: '..count..' jokers across '..page..' pages\n')
        page = nil
    else
        next_page = page + 1
    end
    return label
end
