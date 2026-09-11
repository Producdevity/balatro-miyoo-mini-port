local owners = {}
for name, entry in pairs(G.SVMM_UPDATE_PROFILE or {}) do
    owners[#owners + 1] = {
        name = name,
        calls = entry.calls,
        elapsed = entry.elapsed
    }
end
table.sort(owners, function(a, b) return a.elapsed > b.elapsed end)
local output = {}
for i = 1, #owners do
    local item = owners[i]
    output[#output + 1] = string.format(
        '%s %.1fms/%d', item.name,
        item.elapsed * 1000, item.calls)
end
G.SVMM_UPDATE_PROFILE = {}
return table.concat(output, ', ')
