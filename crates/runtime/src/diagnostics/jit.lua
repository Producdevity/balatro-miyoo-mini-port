local profile = G.SVMM_JIT_PROFILE
if not profile then return '' end
local reasons = {}
for reason, count in pairs(profile.reasons) do
    reasons[#reasons + 1] = {reason = reason, count = count}
end
table.sort(reasons, function(a, b) return a.count > b.count end)
local output = {}
for i = 1, math.min(6, #reasons) do
    output[#output + 1] = reasons[i].reason .. '=' .. reasons[i].count
end
return string.format('started=%d stopped=%d aborted=%d reasons=[%s]',
    profile.started, profile.stopped, profile.aborted,
    table.concat(output, ', '))
