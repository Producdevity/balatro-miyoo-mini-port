local output = {}
if G and G.check then
    for _, kind in ipairs({'update', 'draw'}) do
        local check = G.check[kind]
        if check then
            local parts = {}
            for i = 1, check.checkpoints do
                local item = check.checkpoint_list[i]
                parts[#parts + 1] = string.format('%s %.1fms', item.label, item.average * 1000)
            end
            output[#output + 1] = kind .. ': ' .. table.concat(parts, ', ')
        end
    end
end
return table.concat(output, '\n')
