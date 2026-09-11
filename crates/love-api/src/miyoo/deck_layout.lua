local original_view_deck = G.UIDEF.view_deck
function G.UIDEF.view_deck(...)
    local definition = original_view_deck(...)
    local function fit(node)
        local object = node.config and node.config.object
        if object and object.config and object.config.view_deck and
           object.config.type == 'title' then
            -- Leave space for the deck description and rank totals. Card faces
            -- keep their size; only the spacing between cards changes.
            object.T.w = math.min(object.T.w, G.TILE_W - 6.7)
            object:align_cards()
            object:hard_set_cards()
        end
        for _, child in ipairs(node.nodes or {}) do fit(child) end
    end
    fit(definition)
    return definition
end
