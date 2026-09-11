local function role_list(role_type)
    if role_type == 'Major' then return G.SVMM_MAJORS end
    if role_type == 'Minor' then return G.SVMM_MINORS end
    if role_type == 'Glued' then return G.SVMM_GLUED end
end

function SVMM_untrack_moveable_role(item)
    local list = item._svmm_role_list
    local index = item._svmm_role_index
    if not list or not index or list[index] ~= item then return end

    local last_index = #list
    local last = list[last_index]
    list[index] = last
    list[last_index] = nil
    if last and last ~= item then last._svmm_role_index = index end
    item._svmm_role_list = nil
    item._svmm_role_index = nil
end

function SVMM_track_moveable_role(item)
    local list = role_list(item.role and item.role.role_type)
    if item._svmm_role_list == list then return end
    SVMM_untrack_moveable_role(item)
    if not list then return end

    local index = #list + 1
    list[index] = item
    item._svmm_role_list = list
    item._svmm_role_index = index
end

if Moveable and not Moveable._svmm_role_lists then
    Moveable._svmm_role_lists = true

    local moveable_init = Moveable.init
    Moveable.init = function(self, ...)
        local result = moveable_init(self, ...)
        SVMM_track_moveable_role(self)
        return result
    end

    local moveable_set_role = Moveable.set_role
    Moveable.set_role = function(self, ...)
        local result = moveable_set_role(self, ...)
        SVMM_track_moveable_role(self)
        return result
    end

    local moveable_remove = Moveable.remove
    Moveable.remove = function(self, ...)
        SVMM_untrack_moveable_role(self)
        return moveable_remove(self, ...)
    end
end

local function ensure_role_lists(moveables)
    if not G.SVMM_MAJORS then G.SVMM_MAJORS = {} end
    if not G.SVMM_MINORS then G.SVMM_MINORS = {} end
    if not G.SVMM_GLUED then G.SVMM_GLUED = {} end
    if G._svmm_role_lists_ready then return end

    for i = 1, #moveables do SVMM_track_moveable_role(moveables[i]) end
    G._svmm_role_lists_ready = true
end

local function finish_skipped_move(item, frame, frame_move)
    frame.OLD_MAJOR = frame.MAJOR
    frame.MAJOR = nil
    frame.MOVE = frame_move
    item.NEW_ALIGNMENT = false
end

local function collect_majors(list, frame_move, active, active_count)
    for i = 1, #list do
        local item = list[i]
        local frame = item.FRAME
        if frame.MOVE < frame_move then
            local skip = false
            -- Subclasses can do layout work even when their own bounds are still.
            if item.STATIONARY and Moveable and item.move == Moveable.move then
                local transform, visible = item.T, item.VT
                local velocity, pinch, states = item.velocity, item.pinch, item.states
                local alignment = item.alignment
                skip = not item.juice and not alignment.lr_clamp and
                    alignment.prev_type == alignment.type and
                    not pinch.x and not pinch.y and
                    not (item.zoom and (states.drag.is or states.hover.is)) and
                    transform.x == visible.x and transform.y == visible.y and
                    transform.w == visible.w and transform.h == visible.h and
                    transform.r == visible.r and transform.scale == visible.scale and
                    velocity.x == 0 and velocity.y == 0 and
                    velocity.r == 0 and velocity.scale == 0
            end
            if skip then
                finish_skipped_move(item, frame, frame_move)
            else
                active_count = active_count + 1
                active[active_count] = item
            end
        end
    end
    return active_count
end

local function can_skip_minor(item, frame_move, refresh_major_cache)
    local major = item.role.major
    if not Moveable or item.move ~= Moveable.move or
        not item.STATIONARY or not major or refresh_major_cache then return false end

    local alignment = item.alignment
    local previous, offset = alignment.prev_offset, alignment.offset
    local role_offset = item.role.offset
    local layered = item.layered_parallax
    local refresh_unchanged = not item.config.refresh_movement or
        (item._svmm_refresh_major == major and
            item._svmm_refresh_offset_x == role_offset.x and
            item._svmm_refresh_offset_y == role_offset.y and
            item._svmm_refresh_layered_x == layered.x and
            item._svmm_refresh_layered_y == layered.y)
    local unchanged = not item.NEW_ALIGNMENT and refresh_unchanged and
        not item.juice and not alignment.lr_clamp and previous and
        previous.x == offset.x and previous.y == offset.y and
        alignment.prev_type == alignment.type
    if not unchanged or major.FRAME.MOVE ~= frame_move or not major.STATIONARY then
        return false
    end

    local role = item.role
    if role.xy_bond ~= 'Weak' and role.wh_bond ~= 'Weak' and
        role.r_bond ~= 'Weak' and role.scale_bond ~= 'Weak' then
            return true
    end
    local transform, visible = item.T, item.VT
    if transform.x ~= visible.x or transform.y ~= visible.y or
        transform.w ~= visible.w or transform.h ~= visible.h or
        transform.r ~= visible.r or transform.scale ~= visible.scale then
            return false
    end
    local velocity = item.velocity
    return velocity.x == 0 and velocity.y == 0 and
        velocity.r == 0 and velocity.scale == 0
end

local function collect_minors(list, frame_move, refresh_major_cache, active, active_count)
    for i = 1, #list do
        local item = list[i]
        local frame = item.FRAME
        if frame.MOVE < frame_move then
            if can_skip_minor(item, frame_move, refresh_major_cache) then
                finish_skipped_move(item, frame, frame_move)
            else
                active_count = active_count + 1
                active[active_count] = item
            end
        end
    end
    return active_count
end

local function collect_glued(list, frame_move, glued)
    local count = 0
    for i = 1, #list do
        local item = list[i]
        local frame = item.FRAME
        if frame.MOVE < frame_move then
            local major = item.role.major
            local major_unchanged = major and major.FRAME.MOVE == frame_move and
                ((major.role.role_type == 'Glued' and major._svmm_glued_unchanged) or
                    (major.role.role_type ~= 'Glued' and major.STATIONARY))
            local alignment = item.alignment
            if Moveable and item.move == Moveable.move and
                not alignment.lr_clamp and alignment.prev_type == alignment.type and
                major_unchanged and item._svmm_glued_major == major then
                    finish_skipped_move(item, frame, frame_move)
                    item._svmm_glued_unchanged = true
            else
                count = count + 1
                glued[count] = item
            end
        end
    end
    return count
end

local function move_glued(item, frame_move, dt)
    if item.FRAME.MOVE >= frame_move then return 0 end
    if item._svmm_glued_visiting == frame_move then
        item:move(dt)
        item._svmm_glued_unchanged = false
        return 1
    end

    item._svmm_glued_visiting = frame_move
    local role = item.role
    local major = role.major
    local moved = 0
    if major and major.FRAME.MOVE < frame_move and
        major.role.role_type == 'Glued' then
            moved = move_glued(major, frame_move, dt)
    end

    local major_unchanged = major and major.FRAME.MOVE == frame_move and
        ((major.role.role_type == 'Glued' and major._svmm_glued_unchanged) or
            (major.role.role_type ~= 'Glued' and major.STATIONARY))
    local alignment = item.alignment
    if Moveable and item.move == Moveable.move and
        not alignment.lr_clamp and alignment.prev_type == alignment.type and
        major_unchanged and item._svmm_glued_major == major then
            finish_skipped_move(item, item.FRAME, frame_move)
            item._svmm_glued_unchanged = true
    else
        item:move(dt)
        moved = moved + 1
        item._svmm_glued_unchanged = false
    end
    item._svmm_glued_major = item.role.major
    item._svmm_glued_visiting = nil
    return moved
end

function SVMM_collect_moveables(moveables, frame_move, refresh_major_cache, active, glued)
    ensure_role_lists(moveables)
    local active_count = collect_majors(G.SVMM_MAJORS, frame_move, active, 0)
    active_count = collect_minors(
        G.SVMM_MINORS, frame_move, refresh_major_cache, active, active_count)
    local glued_count = collect_glued(G.SVMM_GLUED, frame_move, glued)
    return active_count, glued_count
end

SVMM_MOVE_SCAN_JIT_FUNCTIONS = {
    majors = collect_majors,
    minors = collect_minors,
    glued = collect_glued,
}

local function reset_collisions(controller)
    if not controller then return end
    local collisions = controller.collision_list
    if collisions then
        for i = 1, #collisions do
            collisions[i].states.collide.is = false
        end
    end
    local dragging = controller.dragging
    if dragging and dragging.target then
        dragging.target.states.collide.is = false
    end
end

local function move_active(list, count, frame_move, dt)
    local moved = 0
    for i = 1, count do
        local item = list[i]
        if item.FRAME.MOVE < frame_move then
            item:move(dt, true)
            moved = moved + 1
        end
        list[i] = nil
    end
    return moved
end

local function moveable_kind(item)
    if Card and item:is(Card) then return 'Card' end
    if CardArea and item:is(CardArea) then return 'CardArea' end
    if DynaText and item:is(DynaText) then return 'DynaText' end
    if UIBox and item:is(UIBox) then return 'UIBox' end
    if UIElement and item:is(UIElement) then return 'UIElement' end
    if AnimatedSprite and item:is(AnimatedSprite) then return 'AnimatedSprite' end
    if Sprite and item:is(Sprite) then return 'Sprite' end
    if Particles and item:is(Particles) then return 'Particles' end
    return 'Moveable'
end

local function move_reason(item, frame_move)
    local role = item.role
    local role_type = role.role_type
    local config = item.config
    local refresh_changed = config.refresh_movement and role_type == 'Minor' and
        (item._svmm_refresh_major ~= role.major or
            item._svmm_refresh_offset_x ~= role.offset.x or
            item._svmm_refresh_offset_y ~= role.offset.y or
            item._svmm_refresh_layered_x ~= item.layered_parallax.x or
            item._svmm_refresh_layered_y ~= item.layered_parallax.y)
    if not item.STATIONARY then return role_type .. ':dynamic' end
    if item.NEW_ALIGNMENT then return role_type .. ':alignment' end
    if refresh_changed then
        if item._svmm_refresh_major ~= role.major then return 'Minor:refresh-major' end
        if item._svmm_refresh_offset_x ~= role.offset.x or
            item._svmm_refresh_offset_y ~= role.offset.y then
                return 'Minor:refresh-offset'
        end
        return 'Minor:refresh-parallax'
    end
    if item.juice then return role_type .. ':juice' end
    if item.alignment.lr_clamp then return role_type .. ':clamp' end
    if item.alignment.prev_type ~= item.alignment.type or
        item.alignment.prev_offset.x ~= item.alignment.offset.x or
        item.alignment.prev_offset.y ~= item.alignment.offset.y then
            return role_type .. ':offset'
    end
    if role_type == 'Minor' and not role.major then return 'Minor:no-major' end
    if role_type == 'Minor' and role.major.FRAME.MOVE < frame_move then
        local parent_role = role.major.role and role.major.role.role_type or 'unknown'
        return 'Minor:stale-' .. parent_role
    end
    if role_type == 'Minor' and not role.major.STATIONARY then
        return 'Minor:active-major'
    end
    if role_type == 'Minor' and (role.xy_bond == 'Weak' or
        role.wh_bond == 'Weak' or role.r_bond == 'Weak' or
        role.scale_bond == 'Weak') then
            return 'Minor:weak-bond'
    end
    return role_type .. ':transform'
end

local function count_move_reasons(list, count, frame_move, reasons)
    for i = 1, count do
        local item = list[i]
        local reason = move_reason(item, frame_move) .. ':' .. moveable_kind(item)
        reasons[reason] = (reasons[reason] or 0) + 1
    end
end

local function remember_refresh_state(item)
    if not item.config.refresh_movement then return end
    local role = item.role
    local layered = item.layered_parallax
    item._svmm_refresh_major = role.major
    item._svmm_refresh_offset_x = role.offset.x
    item._svmm_refresh_offset_y = role.offset.y
    item._svmm_refresh_layered_x = layered.x
    item._svmm_refresh_layered_y = layered.y
end

local function settle_minor_parent(item, frame_move, refresh_major_cache)
    local major = item.role.major
    if not major or major.FRAME.MOVE >= frame_move or
        major.role.role_type ~= 'Minor' or
        major._svmm_minor_visiting == frame_move then
            return
    end

    major._svmm_minor_visiting = frame_move
    settle_minor_parent(major, frame_move, refresh_major_cache)
    if can_skip_minor(major, frame_move, refresh_major_cache) then
        finish_skipped_move(major, major.FRAME, frame_move)
    end
    major._svmm_minor_visiting = nil
end

local function move_minors(list, frame_move, refresh_major_cache, move_dt, reasons)
    local active_count = 0
    local moved = 0
    for i = 1, #list do
        local item = list[i]
        local frame = item.FRAME
        if frame.MOVE < frame_move then
            settle_minor_parent(item, frame_move, refresh_major_cache)
            if can_skip_minor(item, frame_move, refresh_major_cache) then
                finish_skipped_move(item, frame, frame_move)
            else
                active_count = active_count + 1
                if reasons then
                    local reason = move_reason(item, frame_move) .. ':' .. moveable_kind(item)
                    reasons[reason] = (reasons[reason] or 0) + 1
                end
                item:move(move_dt, true)
                moved = moved + 1
                remember_refresh_state(item)
            end
        end
    end
    return active_count, moved
end

SVMM_MOVE_SCAN_JIT_FUNCTIONS.minor_move = move_minors

local function update_registered(self, update_dt)
    local updateables = self.SVMM_UPDATEABLES
    if _SVMM_PHASE_PROFILE then self._svmm_updateable_count = #updateables end
    local i = 1
    while i <= #updateables do
        local item = updateables[i]
        item:update(update_dt)
        item.states.collide.is = false
        if updateables[i] == item then i = i + 1 end
    end
end

function SVMM_update_moveables(self, move_dt, update_dt)
    local moveables = self.MOVEABLES
    ensure_role_lists(moveables)
    local active = self._svmm_active_moveables or {}
    local glued = self._svmm_glued_moveables or {}
    self._svmm_active_moveables = active
    self._svmm_glued_moveables = glued

    local profile_sample = _SVMM_PROFILE_SAMPLE
    local profile_started = profile_sample and love.timer.getTime()
    local collect_ms = 0
    local active_move_ms = 0
    local move_reasons = profile_sample and {}
    local frame_move = G.FRAMES.MOVE

    local major_count = collect_majors(G.SVMM_MAJORS, frame_move, active, 0)
    if profile_sample then
        local now = love.timer.getTime()
        collect_ms = collect_ms + (now - profile_started)*1000
        count_move_reasons(active, major_count, frame_move, move_reasons)
        profile_started = now
    end
    local moved_count = move_active(active, major_count, frame_move, move_dt)
    if profile_sample then
        local now = love.timer.getTime()
        active_move_ms = active_move_ms + (now - profile_started)*1000
        profile_started = now
    end

    local minor_count, minor_moved = move_minors(
        G.SVMM_MINORS, frame_move, not not G.REFRESH_FRAME_MAJOR_CACHE,
        move_dt, move_reasons)
    moved_count = moved_count + minor_moved
    if profile_sample then
        local now = love.timer.getTime()
        active_move_ms = active_move_ms + (now - profile_started)*1000
        profile_started = now
    end

    local glued_count = collect_glued(G.SVMM_GLUED, frame_move, glued)
    if profile_sample then
        local now = love.timer.getTime()
        collect_ms = collect_ms + (now - profile_started)*1000
        self._svmm_collect_ms = collect_ms
        self._svmm_active_move_ms = active_move_ms
        self._svmm_move_reasons = move_reasons
        profile_started = now
    end
    for i = 1, glued_count do
        local item = glued[i]
        moved_count = moved_count + move_glued(item, frame_move, move_dt)
        glued[i] = nil
    end
    if profile_sample then
        local now = love.timer.getTime()
        self._svmm_glued_move_ms = (now - profile_started)*1000
        profile_started = now
    end
    if _SVMM_PHASE_PROFILE then
        self._svmm_active_count = major_count + minor_count + glued_count
        self._svmm_moved_count = moved_count
    end

    timer_checkpoint('move', 'update')
    update_registered(self, update_dt)
    -- Callbacks consume the previous controller pass before collisions reset.
    reset_collisions(self.CONTROLLER)
    if profile_sample then
        self._svmm_updateables_ms = (love.timer.getTime() - profile_started)*1000
    end
end
