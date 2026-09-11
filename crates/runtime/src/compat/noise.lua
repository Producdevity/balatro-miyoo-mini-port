do
    -- Standard Perlin permutation table
    local p = {
        151,160,137,91,90,15,131,13,201,95,96,53,194,233,7,225,
        140,36,103,30,69,142,8,99,37,240,21,10,23,190,6,148,
        247,120,234,75,0,26,197,62,94,252,219,203,117,35,11,32,
        57,177,33,88,237,149,56,87,174,20,125,136,171,168,68,175,
        74,165,71,134,139,48,27,166,77,146,158,231,83,111,229,122,
        60,211,133,230,220,105,92,41,55,46,245,40,244,102,143,54,
        65,25,63,161,1,216,80,73,209,76,132,187,208,89,18,169,
        200,196,135,130,116,188,159,86,164,100,109,198,173,186,3,64,
        52,217,226,250,124,123,5,202,38,147,118,126,255,82,85,212,
        207,206,59,227,47,16,58,17,182,189,28,42,223,183,170,213,
        119,248,152,2,44,154,163,70,221,153,101,155,167,43,172,9,
        129,22,39,253,19,98,108,110,79,113,224,232,178,185,112,104,
        218,246,97,228,251,34,242,193,238,210,144,12,191,179,162,241,
        81,51,145,235,249,14,239,107,49,192,214,31,181,199,106,157,
        184,84,204,176,115,121,50,45,127,4,150,254,138,236,205,93,
        222,114,67,29,24,72,243,141,128,195,78,66,215,61,156,180
    }
    -- Double the table for overflow-free indexing
    local perm = {}
    for i = 0, 255 do
        perm[i] = p[i + 1]
        perm[i + 256] = p[i + 1]
    end

    local function fade(t) return t * t * t * (t * (t * 6 - 15) + 10) end
    local function lerp(t, a, b) return a + t * (b - a) end

    local grad3 = {
        {1,1,0},{-1,1,0},{1,-1,0},{-1,-1,0},
        {1,0,1},{-1,0,1},{1,0,-1},{-1,0,-1},
        {0,1,1},{0,-1,1},{0,1,-1},{0,-1,-1}
    }

    local function dot2(g, x, y) return g[1]*x + g[2]*y end
    local function dot3(g, x, y, z) return g[1]*x + g[2]*y + g[3]*z end

    love.math.noise = function(x, y, z, w)
        if y == nil then
            -- 1D Perlin noise
            local xi = math.floor(x) % 256
            local xf = x - math.floor(x)
            local u = fade(xf)
            local a = perm[xi] % 2 == 0 and xf or -xf
            local b = perm[xi + 1] % 2 == 0 and (xf - 1) or -(xf - 1)
            return lerp(u, a, b) * 0.5 + 0.5
        elseif z == nil then
            -- 2D Perlin noise
            local xi = math.floor(x) % 256
            local yi = math.floor(y) % 256
            local xf = x - math.floor(x)
            local yf = y - math.floor(y)
            local u = fade(xf)
            local v = fade(yf)
            local aa = perm[perm[xi] + yi]
            local ab = perm[perm[xi] + yi + 1]
            local ba = perm[perm[xi + 1] + yi]
            local bb = perm[perm[xi + 1] + yi + 1]
            local gaa = grad3[(aa % 12) + 1]
            local gab = grad3[(ab % 12) + 1]
            local gba = grad3[(ba % 12) + 1]
            local gbb = grad3[(bb % 12) + 1]
            local val = lerp(v,
                lerp(u, dot2(gaa, xf, yf), dot2(gba, xf - 1, yf)),
                lerp(u, dot2(gab, xf, yf - 1), dot2(gbb, xf - 1, yf - 1))
            )
            return val * 0.5 + 0.5
        else
            -- 3D Perlin noise
            local xi = math.floor(x) % 256
            local yi = math.floor(y) % 256
            local zi = math.floor(z or 0) % 256
            local xf = x - math.floor(x)
            local yf = y - math.floor(y)
            local zf = (z or 0) - math.floor(z or 0)
            local u = fade(xf)
            local v = fade(yf)
            local w = fade(zf)
            local aaa = perm[perm[perm[xi] + yi] + zi]
            local aba = perm[perm[perm[xi] + yi + 1] + zi]
            local aab = perm[perm[perm[xi] + yi] + zi + 1]
            local abb = perm[perm[perm[xi] + yi + 1] + zi + 1]
            local baa = perm[perm[perm[xi + 1] + yi] + zi]
            local bba = perm[perm[perm[xi + 1] + yi + 1] + zi]
            local bab = perm[perm[perm[xi + 1] + yi] + zi + 1]
            local bbb = perm[perm[perm[xi + 1] + yi + 1] + zi + 1]
            local gaaa = grad3[(aaa % 12) + 1]
            local gaba = grad3[(aba % 12) + 1]
            local gaab = grad3[(aab % 12) + 1]
            local gabb = grad3[(abb % 12) + 1]
            local gbaa = grad3[(baa % 12) + 1]
            local gbba = grad3[(bba % 12) + 1]
            local gbab = grad3[(bab % 12) + 1]
            local gbbb = grad3[(bbb % 12) + 1]
            local val = lerp(w,
                lerp(v,
                    lerp(u, dot3(gaaa,xf,yf,zf), dot3(gbaa,xf-1,yf,zf)),
                    lerp(u, dot3(gaba,xf,yf-1,zf), dot3(gbba,xf-1,yf-1,zf))
                ),
                lerp(v,
                    lerp(u, dot3(gaab,xf,yf,zf-1), dot3(gbab,xf-1,yf,zf-1)),
                    lerp(u, dot3(gabb,xf,yf-1,zf-1), dot3(gbbb,xf-1,yf-1,zf-1))
                )
            )
            return val * 0.5 + 0.5
        end
    end
end
