-- The only thing the logic does: count the notices, wake her up, and hand out
-- the workspaces the compositor reports. None of this paints: what is seen is
-- moved by the renderer, and it keeps moving even if this stops to think.

local total = 0

local function set(n)
    total = n
    fact.pending = n
    text.pending_label = n > 9 and "9+" or tostring(n)
end

set(3)
-- Let her be seen coming in: she is born off the screen and comes down on her own with her spring.
after(120, function() fact.awake = true end)
every(6000, function() set((total % 12) + 1) end)

-- ── the workspaces ───────────────────────────────────────────────
-- `sys.watch` says whether this system has them. If not, the card comes out without them.
sys.watch("workspaces", function(w)
    local exists, busy = {}, {}
    for _, d in ipairs(w.list) do
        exists[d.id] = true
        busy[d.id] = d.windows > 0
    end
    for i = 1, 9 do
        fact["ws." .. i .. ".there"] = exists[i] or i == w.active
        fact["ws." .. i .. ".busy"] = busy[i] or false
    end
    fact["ws.active"] = w.active
end)

-- Pressing one takes you there.
on("go", function(i) sys.call("workspaces.focus", i) end)
