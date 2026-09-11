-- Skip tutorial — tutorial events block ROUND_EVAL progression,
-- and tutorial check forces seed="TUTORIAL" (same game every time).
-- Must set tutorial_complete=true BEFORE start_run so the seed line
-- sees it and calls generate_starting_seed() instead.
local _orig_start_run = Game and Game.start_run
if _orig_start_run then
    Game.start_run = function(self, args)
        -- Set BEFORE start_run so seed generation uses random seed
        if G and G.SETTINGS then
            G.SETTINGS.tutorial_complete = true
            G.SETTINGS.tutorial_progress = G.SETTINGS.tutorial_progress or {}
            G.SETTINGS.tutorial_progress.completed_parts = G.SETTINGS.tutorial_progress.completed_parts or {}
            G.SETTINGS.tutorial_progress.completed_parts.big_blind = true
        end
        local ret = _orig_start_run(self, args)
        -- Also clear tutorial event queue after run starts
        if G and G.E_MANAGER and G.E_MANAGER.queues and G.E_MANAGER.queues.tutorial then
            G.E_MANAGER.queues.tutorial = {}
        end
        return ret
    end
end

-- Patch end_round to fix event that never returns true.
-- In Balatro's state_events.lua:159-171, the win_notified event only returns
-- true when G.STATE==ROUND_EVAL, returning nil otherwise (hanging forever).
local _orig_end_round = end_round
if _orig_end_round then
    end_round = function(...)
        local ret = _orig_end_round(...)
        -- Fix: if there's a stuck immediate event in base queue, patch its func
        if G and G.E_MANAGER and G.E_MANAGER.queues and G.E_MANAGER.queues.base then
            local bq = G.E_MANAGER.queues.base
            for i = #bq, math.max(1, #bq - 3), -1 do
                local ev = bq[i]
                if ev and ev.trigger == 'immediate' and ev.blocking == false
                   and ev.blockable == false and ev.func then
                    local orig_f = ev.func
                    ev.func = function(self)
                        local r = orig_f(self)
                        if r then return true end
                        return true  -- always complete
                    end
                end
            end
        end
        return ret
    end
end
