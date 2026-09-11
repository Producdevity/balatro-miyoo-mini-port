love.math.newRandomGenerator = function(low, high)
    local _state = 1
    local _seed_low = 0
    local _seed_high = 0

    local rng = {}
    local rng_mt = {__index = rng}
    setmetatable(rng, rng_mt)

    function rng:setSeed(lo, hi)
        lo = lo or 0
        hi = hi or 0
        _seed_low = lo
        _seed_high = hi
        -- Combine into seed
        local s = math.floor(math.abs(lo))
        if hi ~= 0 then
            s = s + math.floor(math.abs(hi)) * 65536
        end
        s = s % 2147483646 + 1  -- Ensure [1, 2147483646]
        _state = s
    end

    function rng:getSeed()
        return _seed_low, _seed_high
    end

    local function next_rand()
        -- Park-Miller Minimal Standard PRNG
        _state = (_state * 16807) % 2147483647
        return _state / 2147483647
    end

    function rng:random(a, b)
        if a == nil then
            return next_rand()
        elseif b == nil then
            if type(a) == 'number' then
                return math.floor(next_rand() * a) + 1
            end
            return next_rand()
        else
            return math.floor(next_rand() * (b - a + 1)) + a
        end
    end

    function rng:randomNormal(stddev, mean)
        stddev = stddev or 1
        mean = mean or 0
        local u1 = next_rand()
        local u2 = next_rand()
        if u1 < 1e-10 then u1 = 1e-10 end
        local z = math.sqrt(-2 * math.log(u1)) * math.cos(2 * math.pi * u2)
        return z * stddev + mean
    end

    function rng:getState()
        return tostring(_state)
    end

    function rng:setState(s)
        _state = tonumber(s) or 1
    end

    -- Seed from constructor args
    if low then
        rng:setSeed(low, high)
    else
        rng:setSeed(os.time())
    end

    return rng
end

-- Fix love.math.getRandomSeed to return actual seed info
love.math.getRandomSeed = function()
    return os.time(), 0
end
