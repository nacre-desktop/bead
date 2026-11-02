-- SPDX-License-Identifier: MIT
return function(api)
    return {render=function(tokens)
        return api.ui.Text{id="hello",text="Hello from a plugin",height=24,font_size=tokens.font_size,color="onSurfaceVariant"}
    end}
end
