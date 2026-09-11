local function count(list)
    local total = 0
    if list then
        for _ in pairs(list) do total = total + 1 end
    end
    return total
end
local roles = {Major = 0, Minor = 0, Glued = 0, Other = 0}
local stationary_roles = {Major = 0, Minor = 0, Glued = 0, Other = 0}
local stationary = 0
local stable_major = 0
local stable_minor = 0
local stable_strong_minor = 0
local stable_glued = 0
local glued_unchanged = 0
local glued_major_current = 0
local glued_same_major = 0
local plain_ui_text = 0
local plain_ui_panel = 0
if G and G.MOVEABLES then
    for _, item in pairs(G.MOVEABLES) do
        local role = item.role and item.role.role_type or 'Other'
        roles[role] = (roles[role] or 0) + 1
        if item.STATIONARY then
            stationary = stationary + 1
            stationary_roles[role] = (stationary_roles[role] or 0) + 1
        end
        local velocity = item.velocity or {}
        local transform = item.T or {}
        local visible = item.VT or {}
        local stable_transform = transform.x == visible.x
            and transform.y == visible.y
            and transform.w == visible.w
            and transform.h == visible.h
            and transform.r == visible.r
            and transform.scale == visible.scale
            and (velocity.x or 0) == 0
            and (velocity.y or 0) == 0
            and (velocity.r or 0) == 0
            and (velocity.scale or 0) == 0
        local inactive = not item.NEW_ALIGNMENT
            and not item.juice
            and not (item.config and item.config.refresh_movement)
            and not (item.states and item.states.drag and item.states.drag.is)
            and not (item.zoom and item.states and item.states.hover and item.states.hover.is)
        if role == 'Major' and item.STATIONARY and stable_transform and inactive then
            stable_major = stable_major + 1
        elseif role == 'Minor' and item.STATIONARY and stable_transform and inactive
            and item.role.major and item.role.major.STATIONARY then
            stable_minor = stable_minor + 1
            if item.role.xy_bond == 'Strong'
                and item.role.wh_bond == 'Strong'
                and item.role.r_bond == 'Strong'
                and item.role.scale_bond == 'Strong' then
                stable_strong_minor = stable_strong_minor + 1
            end
        elseif role == 'Glued' and item.role.major and item.role.major.STATIONARY then
            stable_glued = stable_glued + 1
        end
        if role == 'Glued' then
            if item._svmm_glued_unchanged then glued_unchanged = glued_unchanged + 1 end
            if item.role.major and item.role.major.FRAME.MOVE == G.FRAMES.MOVE then
                glued_major_current = glued_major_current + 1
            end
            if item._svmm_glued_major == item.role.major then
                glued_same_major = glued_same_major + 1
            end
        end
        if item._svmm_plain_ui == 1 then
            plain_ui_text = plain_ui_text + 1
        elseif item._svmm_plain_ui == 2 then
            plain_ui_panel = plain_ui_panel + 1
        end
    end
end
local callbacks = {}
if G and G.ARGS and G.ARGS.FUNC_TRACKER then
    for name, calls in pairs(G.ARGS.FUNC_TRACKER) do
        callbacks[#callbacks + 1] = {name = name, calls = calls}
    end
    table.sort(callbacks, function(a, b) return a.calls > b.calls end)
end
local callback_parts = {}
for i = 1, math.min(8, #callbacks) do
    callback_parts[#callback_parts + 1] = callbacks[i].name .. '=' .. callbacks[i].calls
end
local move_reasons = {}
for name, calls in pairs(G and G._svmm_move_reasons or {}) do
    move_reasons[#move_reasons + 1] = {name = name, calls = calls}
end
table.sort(move_reasons, function(a, b)
    if a.calls == b.calls then return a.name < b.name end
    return a.calls > b.calls
end)
local move_reason_parts = {}
for i = 1, #move_reasons do
    move_reason_parts[#move_reason_parts + 1] =
        move_reasons[i].name .. '=' .. move_reasons[i].calls
end
local drawhash_count = #(G and G.DRAW_HASH or {})
local drawhash_unique = 0
local drawhash_interactive = 0
local seen = {}
for _, item in ipairs(G and G.DRAW_HASH or {}) do
    if not seen[item] then
        seen[item] = true
        drawhash_unique = drawhash_unique + 1
        local states = item.states or {}
        if (states.hover and states.hover.can)
            or (states.collide and states.collide.can)
            or (states.drag and states.drag.can) then
            drawhash_interactive = drawhash_interactive + 1
        end
    end
end
return string.format(
    'moveables=%d active=%d moved=%d updateables=%d move_ms=%.2f/%.2f/%.2f/%.2f stationary=%d roles=%d/%d/%d/%d stationary_roles=%d/%d/%d/%d stable=%d/%d/%d strong_minor=%d glued=%d/%d/%d plain_ui=%d/%d nodes=%d uiboxes=%d cards=%d cardareas=%d drawhash=%d/%d/%d move_reasons=[%s] callbacks=[%s]',
    count(G and G.MOVEABLES),
    G and G._svmm_active_count or 0,
    G and G._svmm_moved_count or 0,
    G and G._svmm_updateable_count or 0,
    G and G._svmm_collect_ms or 0,
    G and G._svmm_active_move_ms or 0,
    G and G._svmm_glued_move_ms or 0,
    G and G._svmm_updateables_ms or 0,
    stationary,
    roles.Major or 0, roles.Minor or 0, roles.Glued or 0, roles.Other or 0,
    stationary_roles.Major or 0, stationary_roles.Minor or 0,
    stationary_roles.Glued or 0, stationary_roles.Other or 0,
    stable_major, stable_minor, stable_glued,
    stable_strong_minor,
    glued_unchanged, glued_major_current, glued_same_major,
    plain_ui_text, plain_ui_panel,
    count(G and G.I and G.I.NODE),
    count(G and G.I and G.I.UIBOX),
    count(G and G.I and G.I.CARD),
    count(G and G.I and G.I.CARDAREA),
    drawhash_count, drawhash_unique, drawhash_interactive,
    table.concat(move_reason_parts, ', '),
    table.concat(callback_parts, ', ')
)
