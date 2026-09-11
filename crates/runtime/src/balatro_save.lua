if not (G and G.SAVE_MANAGER and G.SAVE_MANAGER.channel) then
    return
end

local channel = G.SAVE_MANAGER.channel
local responses = {}

local function profile_dir(profile)
    local path = tostring(profile or 1)
    if not love.filesystem.getInfo(path) then
        assert(love.filesystem.createDirectory(path))
    end
    return path .. '/'
end

local function save_progress(request)
    local progress = assert(request.save_progress)
    local prefix = profile_dir(progress.SETTINGS.profile)
    local meta_path = prefix .. 'meta.jkr'
    if not love.filesystem.getInfo(meta_path) then
        assert(love.filesystem.append(meta_path, 'return {}'))
    end

    local meta = STR_UNPACK(get_compressed(meta_path) or 'return {}')
    meta.unlocked = meta.unlocked or {}
    meta.discovered = meta.discovered or {}
    meta.alerted = meta.alerted or {}
    local changed = false
    for key, value in pairs(progress.UDA) do
        if string.find(value, 'u') and not meta.unlocked[key] then
            meta.unlocked[key] = true
            changed = true
        end
        if string.find(value, 'd') and not meta.discovered[key] then
            meta.discovered[key] = true
            changed = true
        end
        if string.find(value, 'a') and not meta.alerted[key] then
            meta.alerted[key] = true
            changed = true
        end
    end
    if changed then
        compress_and_save(meta_path, STR_PACK(meta))
    end
    compress_and_save('settings.jkr', progress.SETTINGS)
    compress_and_save(prefix .. 'profile.jkr', progress.PROFILE)
    responses[#responses + 1] = 'done'
end

local function handle(request)
    if type(request) ~= 'table' then
        return
    end
    if request.type == 'save_progress' then
        save_progress(request)
    elseif request.type == 'save_settings' then
        local prefix = profile_dir(request.profile_num)
        compress_and_save('settings.jkr', request.save_settings)
        compress_and_save(prefix .. 'profile.jkr', request.save_profile)
    elseif request.type == 'save_metrics' then
        compress_and_save('metrics.jkr', request.save_metrics)
    elseif request.type == 'save_notify' then
        local prefix = profile_dir(request.profile_num)
        local path = prefix .. 'unlock_notify.jkr'
        if not love.filesystem.getInfo(path) then
            assert(love.filesystem.append(path, ''))
        end
        local notifications = get_compressed(path) or ''
        if request.save_notify and not string.find(notifications, request.save_notify, 1, true) then
            compress_and_save(path, notifications .. request.save_notify .. '\n')
        end
    elseif request.type == 'save_run' then
        local prefix = profile_dir(request.profile_num)
        compress_and_save(prefix .. 'save.jkr', request.save_table)
    end
end

function channel:push(request)
    handle(request)
    return true
end

function channel:supply(request)
    return self:push(request)
end

function channel:pop()
    if #responses == 0 then
        return nil
    end
    return table.remove(responses, 1)
end

function channel:demand()
    return self:pop()
end

function channel:peek()
    return responses[1]
end

function channel:getCount()
    return #responses
end

function channel:clear()
    responses = {}
end
