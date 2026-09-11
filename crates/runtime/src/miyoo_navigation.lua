local capture_input = Controller.capture_focused_input
function Controller:capture_focused_input(button, input_type, dt)
    self.miyoo_cycle_press = input_type == 'press' and
        (button == 'dpleft' or button == 'dpright' or
         button == 'leftshoulder' or button == 'rightshoulder')
    local captured = capture_input(self, button, input_type, dt)
    self.miyoo_cycle_press = nil
    return captured
end

local click = UIElement.click
function UIElement:click()
    -- Each queued controller press is intentional. Keep the mouse debounce and
    -- all visibility, disabled-button and one-press checks in the original click.
    if self.config.button == 'option_cycle' and G.CONTROLLER.miyoo_cycle_press then
        self.last_clicked = nil
    end
    return click(self)
end
