if Card and Card.flip then
    Card.flip = function(self)
        if self.sprite_facing == 'front' then
            self.sprite_facing = 'back'
            self.facing = 'back'
        else
            self.sprite_facing = 'front'
            self.facing = 'front'
        end
        -- Immediately toggle children visibility
        if self.children then
            local is_front = (self.sprite_facing == 'front')
            if self.children.front then self.children.front.states.visible = is_front end
            if self.children.back then self.children.back.states.visible = not is_front end
        end
    end
end
