
if os.getenv('BALATRO_SCALAR_COLLISION') ~= '0' then
    local abs, cos, sin = math.abs, math.cos, math.sin
    local half_pi = math.pi/2
    local zero_cos, zero_sin = cos(half_pi), sin(half_pi)

    function Node:collides_with_point(point)
        local container = self.container
        if not container then return end
        local T = self.CT or self.T
        local x, y = point.x, point.y
        local buffer = self.states.hover.is and G.COLLISION_BUFFER or 0

        if container ~= self then
            local parent = container.T
            if abs(parent.r) < 0.1 then
                x, y = x - parent.w/2, y - parent.h/2
                local c, s = zero_cos, zero_sin
                if parent.r ~= 0 then
                    c, s = cos(parent.r + half_pi), sin(parent.r + half_pi)
                end
                x, y = -y*c + x*s, y*s + x*c
                x, y = x + (parent.w/2 - parent.x), y + (parent.h/2 - parent.y)
            else
                x, y = x - parent.x, y - parent.y
            end
        end

        if abs(T.r) >= 0.1 then
            local c, s = cos(T.r + half_pi), sin(T.r + half_pi)
            x, y = x - (T.x + 0.5*T.w), y - (T.y + 0.5*T.h)
            x, y = y*c - x*s, y*s + x*c
            x, y = x + (T.x + 0.5*T.w), y + (T.y + 0.5*T.h)
        end

        if x >= T.x - buffer and y >= T.y - buffer and
            x <= T.x + T.w + buffer and y <= T.y + T.h + buffer then
            return true
        end
    end
end
