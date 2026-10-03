-- The ball's logic.
--
-- The scene draws and reacts on its own: the highlight on hover, the springs,
-- the gestures and the rules are all carried by the renderer, and if this gets
-- stuck the screen never finds out. The only thing here is what a rule CANNOT
-- do: ask a service for something.

local on_halley = false
for desktop in (sys.ask("env", "XDG_CURRENT_DESKTOP") or ""):gmatch("[^:]+") do
    if desktop:lower() == "halley" then on_halley = true end
end
fact.compositor_halley = on_halley

local function window_call(action, id)
    if on_halley then
        run("marea-halley", { "window", action, tostring(id) }, function(_, code)
            if code ~= 0 then log("Halley · could not " .. action .. " window " .. tostring(id)) end
        end)
    else
        pcall(sys.call, "window." .. action, id)
    end
end

-- ── the two levels ─────────────────────────────────────────────────
--  The scene sends where the finger has gone, from 0 to 1. The value is not
--  kept: the service keeps saying it, so raising the volume with the keyboard
--  key moves this same track without anyone counting it twice.
on("set_volume", function(v) sys.call("audio.volume", v) end)
on("set_brightness", function(v) sys.call("brightness.level", v) end)
on("mute_output", function() sys.call("audio.mute") end)
on("mute_mic", function() sys.call("audio.input_mute") end)
on("set_mic", function(v) sys.call("audio.input", v) end)
--  The keyboard's keys: a step of 5 %. The volume never past 100 % (the
--  service's step stops there); the meter beside her shows it as always.
on("volume_up", function() sys.call("audio.step", 0.05) end)
on("volume_down", function() sys.call("audio.step", -0.05) end)
local function bright_step(d)
    local now = tonumber(fact["display.level"]) or 0.5
    sys.call("brightness.level", math.clamp(now + d, 0.01, 1))
end
on("bright_up", function() bright_step(0.05) end)
on("bright_down", function() bright_step(-0.05) end)

-- ── what is playing ────────────────────────────────────────────────
on("play_pause", function() sys.call("media.toggle") end)
on("previous", function() sys.call("media.previous") end)
on("next", function() sys.call("media.next") end)

-- ── the three cards ────────────────────────────────────────────────
--  Only two carry a switch. Concentration does not: it is Marea's, it is set
--  from its page —where you choose how long it lasts— and the whole scene
--  carries it, without going through here. Wi-Fi and Bluetooth do, and they
--  are the system's: pleamar knows how to READ the network but not how to
--  change it, and it knows nothing about bluetooth yet. A switch that moves
--  without doing anything is a lie, so these do something.
--  The Wi-Fi and Bluetooth switches: what the system says, read from
--  pleamar's own services (NetworkManager and BlueZ, spoken to directly), and
--  flipped from what they say rather than from what was assumed.
local net_state, bt_state = {}, {}

on("flip", function(k)
    if k == 1 then
        pcall(sys.call, "network.wifi", not net_state.wifi)
    elseif k == 2 then
        pcall(sys.call, "bluetooth.power", not bt_state.powered)
    end
end)

-- ── sound: where it plays and where it listens ─────────────────────
--  pleamar says which devices there are —`outputs` and `inputs`, each with its
--  id, its name and whether it is the current one— and accepts
--  `audio.default(id)` to change. Here they are only split into two lists and
--  the name is trimmed, since PipeWire's come with the whole card model in it.
local function shorten(name)
    --  «TU106 High Definition Audio Controller Estéreo digital (HDMI)» does not
    --  fit in a row, and besides, what tells them apart is at the end.
    local clean = tostring(name):gsub("%s+", " ")
    if #clean > 42 then clean = clean:sub(1, 41) .. "…" end
    return clean
end

local function devices(state)
    local outputs, mics = {}, {}
    for _, a in ipairs(state.outputs or {}) do
        outputs[#outputs + 1] = { name = shorten(a.name), id = a.id, current = a.default == true }
    end
    for _, a in ipairs(state.inputs or {}) do
        mics[#mics + 1] = { name = shorten(a.name), id = a.id, current = a.default == true }
    end
    model.outputs = outputs
    model.mics = mics
end

sys.watch("audio", devices)

on("pick_device", function(i)
    --  The two lists go together: the first ones are outputs and the rest
    --  mics, which is how they are seen on the page.
    local outputs = model.outputs or {}
    local chosen = outputs[i + 1]
    if chosen == nil then
        local mics = model.mics or {}
        chosen = mics[i - #outputs + 1]
    end
    if chosen ~= nil then sys.call("audio.default", chosen.id) end
end)

-- ── the pages ──────────────────────────────────────────────────────
--  The networks and the gadgets come from pleamar's services, which also
--  join, pair and forget. A new network with a password asks for it in place;
--  one already known, or open, joins at once. Whichever is being joined
--  breathes until the system says it is the one in use.
local joining_name, joining_since = nil, 0

local function paint_networks()
    local v = {}
    for _, n in ipairs(net_state.networks or {}) do
        v[#v + 1] = { name = n.ssid, signal = n.strength or 0, current = n.active == true, secure = n.secure == true, known = n.known == true }
    end
    table.sort(v, function(a, b)
        if a.current ~= b.current then return a.current end
        if a.known ~= b.known then return a.known end
        return a.signal > b.signal
    end)
    while #v > 8 do table.remove(v) end
    model.networks = v
    fact["switched_on.1"] = net_state.wifi == true
    --  The one being joined, by its place now (the list moves as it is joined).
    fact.joining = -1
    if joining_name ~= nil then
        for k, n in ipairs(v) do
            if n.name == joining_name then
                if n.current then joining_name = nil else fact.joining = k - 1 end
            end
        end
    end
end

sys.watch("network", function(state)
    net_state = state or {}
    paint_networks()
end)

--  And if it never joins —a wrong password, out of reach— it stops breathing.
every(3000, function()
    if joining_name ~= nil and os.time() - joining_since > 25 then
        log("wi-fi · could not join", joining_name)
        joining_name = nil
        paint_networks()
    end
end)

local function join(name, password)
    joining_name, joining_since = name, os.time()
    local ok, err = pcall(sys.call, "network.connect", name, password)
    if not ok then
        log("wi-fi · joining failed:", err)
        joining_name = nil
    end
    paint_networks()
end

on("pick_network", function(i)
    local r = model.networks[i + 1]
    if r == nil or r.current then return end
    if r.secure and not r.known then
        text["ask.name"] = r.name
        text.wifi_pw = ""
        fact.asking = i
        return
    end
    fact.asking = -1
    join(r.name, nil)
end)

on("join", function()
    local r = model.networks[(fact.asking or -1) + 1]
    local password = text.wifi_pw or ""
    fact.asking = -1
    text.wifi_pw = ""
    if r ~= nil and password ~= "" then join(r.name, password) end
end)
on("forget_ask", function()
    fact.asking = -1
    text.wifi_pw = ""
end)

--  Bluetooth: the paired ones first —the connected at the top— and, while
--  looking, the ones just found, to be paired with a press.
local function paint_gadgets()
    local v = {}
    for _, d in ipairs(bt_state.devices or {}) do
        v[#v + 1] = { name = d.name, current = d.connected == true, paired = d.paired == true, battery = d.battery or -1, address = d.address }
    end
    while #v > 8 do table.remove(v) end
    model.gadgets = v
    fact["switched_on.2"] = bt_state.powered == true
    fact.bt_scanning = bt_state.discovering == true
    local first = v[1]
    text["bt.sub"] = (first and first.current) and first.name or (bt_state.powered and tr("On") or tr("Off"))
end

sys.watch("bluetooth", function(state)
    bt_state = state or {}
    paint_gadgets()
end)

on("pick_gadget", function(i)
    local c = model.gadgets[i + 1]
    if c == nil or c.address == nil then return end
    pcall(sys.call, c.current and "bluetooth.disconnect" or "bluetooth.connect", c.address)
end)

on("bt_scan", function()
    pcall(sys.call, "bluetooth.scan", not bt_state.discovering)
end)

--  Opening a page asks for a fresh look: the networks around, and the gadgets
--  near (it listens while the page is open, and stops when it closes).
on("scan", function(k)
    if k == 1 then pcall(sys.call, "network.scan")
    elseif k == 2 and bt_state.powered and not bt_state.discovering then pcall(sys.call, "bluetooth.scan", true) end
end)
on("fact:page", function(p)
    if p ~= "bluetooth" and bt_state.discovering then pcall(sys.call, "bluetooth.scan", false) end
    if p ~= "wifi" then fact.asking = -1 end
end)


-- ── the finder ─────────────────────────────────────────────────────
--  «Apps, files and folders.» The apps come from the system —the `apps`
--  service reads the `.desktop` files, with their icon and their order—, and
--  the files and folders are searched by `fd` under your home. The scene knows
--  nothing of this: it asks for a list and paints it.
local apps = {}
local home_dir = sys.ask("env", "HOME") or "."

--  What is written changes as soon as a key is pressed, but `fd` takes its
--  time: the query it was launched with is noted down and, when it comes back,
--  it is thrown away if it is no longer the current one. Without this, typing
--  fast leaves the list from three letters ago.
local generation = 0

local function paint(q, list)
    local out = {}
    for _, e in ipairs(list) do
        if #out < 7 then out[#out + 1] = e end
    end
    model.results = out
    fact.typing = q ~= ""
    fact.selected = 0
    --  And let the scene know there is a new list, to stagger its entrance.
    emit("list_ready")
end

--  «At hand» is not «the first ones in alphabetical order»: it is the usual
--  ones. With no usage data, this is the most honest thing that can be done —a
--  short list of the ones almost everybody has at hand— and the day there are
--  usage counts, it is changed here and that is it.
local AT_HAND = { "firefox", "zen", "nautilus", "files", "archivos", "kitty", "alacritty", "terminal", "code", "gimp" }

local function prefer(a, b)
    local function rank(x)
        local n = x.name:lower()
        for k, v in ipairs(AT_HAND) do
            if n:find(v, 1, true) then return k end
        end
        return 99
    end
    local pa, pb = rank(a), rank(b)
    if pa ~= pb then return pa < pb end
    return a.name < b.name
end

--  What is hers, findable by writing: her pages and her tricks. Each one with
--  the words it answers to, in both languages whatever the one she speaks —you
--  think «cupos» even with her in English—, the little face it shows in the
--  list (1 plain, 2 happy, 3 angry, 4 waves) and what opening it does. Whatever
--  is built for her from now on goes in here, and it can be found.
local function open_page(page, section)
    return function()
        fact.open = true
        fact.page = page
        if section then fact.section = section end
    end
end
--  What is written further down and opened from here: filled in there.
local hooks = {}
local OWN = {
    { "Calendar", "calendar calendario agenda events eventos evento cita citas reminder recordatorio meeting reunion", 1,
      function() if hooks.calendar then hooks.calendar() end; open_page("calendar")() end },
    { "Agents", "agents agentes claude codex quota quotas cupos cupo reservas limits limites usage uso ia ai tokens", 4,
      function() open_page("agents")(); emit("agents_refresh") end },
    { "Settings", "settings ajustes preferences preferencias configuracion configuration", 1, open_page("settings", "menu") },
    { "Where she lives", "monitor pantalla screen casa home follow seguir sigueme sígueme donde vive", 1, open_page("settings", "home") },
    { "Language", "language idioma lengua english ingles inglés español spanish", 1, open_page("settings", "language") },
    { "Appearance", "appearance apariencia skin piel theme tema glass cristal lens lente look aspecto", 1, open_page("settings", "look") },
    { "Her sky", "sky cielo weather tiempo clima temperatura temperature lluvia rain ciudad city ubicacion ubicación location", 1,
      function() open_page("calendar")(); fact["sky.editing"] = true end },
    { "Her wardrobe", "wardrobe armario ropa clothes hat hats sombrero gorro accesorios accessories headphones cascos auriculares", 2, open_page("wardrobe") },
    { "Wi-Fi", "wifi wi-fi red network internet", 1, open_page("wifi") },
    { "Bluetooth", "bluetooth auriculares headphones", 1, open_page("bluetooth") },
    { "Sound", "sound sonido audio volume volumen output salida microfono microphone", 1, open_page("sound") },
    { "Focus", "focus concentracion concentración pomodoro haven refugio", 1, open_page("concentration") },
    { "Session", "session sesion sesión apagar power poweroff reiniciar reboot restart logout salir suspend suspender", 1, open_page("session") },
    { "Lock the screen", "lock bloquear bloqueo pantalla screen", 1, function() emit("lock") end },
    { "Sleep", "sleep dormir siesta nap", 1, function() emit("sleep") end },
    { "Screenshot of a region", "screenshot captura region región recorte shot", 1, function() emit("shot_region") end },
    { "Screenshot of the screen", "screenshot captura pantalla screen shot", 1, function() emit("shot_screen") end },
    { "Screenshot of a window", "screenshot captura ventana window shot", 1, function() emit("shot_window") end },
    { "Record the screen", "record grabar grabacion grabación video vídeo screencast", 1, function() emit("record_toggle") end },
    --  Only in pleamar-wm's own session: its window manager.
    { "Tiled or free windows", "windows ventanas libres free tiled mosaico tiling floating flotantes modo mode", 1,
      function() if hooks.wm_free then hooks.wm_free() end end, "wm" },
    { "Bring back every window", "minimized minimizadas recoger restore restaurar devolver traer windows ventanas", 1,
      function() if hooks.wm_restore then hooks.wm_restore() end end, "wm" },
    { "Rain on the windows", "rain lluvia llover llueve water agua drops gotas weather tiempo shader windows ventanas", 1,
      function() if hooks.wm_rain then hooks.wm_rain() end end, "wm" },
    { "Rain intensity", "rain lluvia intensity intensidad light suave medium media heavy fuerte drizzle llovizna downpour chaparron chaparrón gotas drops", 1,
      function() if hooks.wm_rain_level then hooks.wm_rain_level() end end, "wm" },
    { "Snow on the windows", "snow nieve nevar nieva winter invierno frost escarcha hielo ice weather tiempo shader windows ventanas", 1,
      function() if hooks.wm_snow then hooks.wm_snow() end end, "wm" },
    { "Ride a window", "ride montar subir súbete subete sit sentarse ventana window marea viaje", 1,
      function() if hooks.wm_ride then hooks.wm_ride() end end, "wm" },
    { "Wallpapers", "wallpaper wallpapers fondo fondos pantalla escritorio desktop background imagen tema theme", 1, function() emit("show_walls") end },
    { "Deriva", "deriva drift biblioteca library guardado saved kept enlaces links recortes bookmarks marcadores", 1, function() emit("show_drift") end },
    { "Do not disturb", "do not disturb no molestar dnd silencio silence quiet callar avisos notices notificaciones notifications", 1,
      function() if hooks.dnd then hooks.dnd() end end },
    { "Notices", "notices avisos notificaciones notifications silenciar mute muted apps posponer snooze later luego grabar recording", 1, open_page("settings", "notices") },
    { "Notice history", "history historial avisos notificaciones notices notifications pasados old viejos marea oleaje swell", 1, function() emit("show_history") end },
    { "Celebrate", "celebrate celebrar fiesta party fireworks cohetes fuegos", 2, function() emit("celebrate") end },
    { "Rage", "rage enfado enfadada angry pistola gun disparar shoot", 3, function() emit("rage") end },
    { "Adventure", "adventure aventura alas wings volar fly saltar", 2, function() emit("adventure") end },
}

--  The windows open right now (the `window` service, further down, keeps
--  this): the finder finds them too, and choosing one goes to it —in
--  pleamar-wm, to the workspace it is on—.
local open_windows = {}

--  Hers first, when what is written starts one of her words or is in her
--  name: typing «cla» finds the agents before any application.
local function own_matches(q)
    local found = {}
    if #q < 2 then return found end
    for k, o in ipairs(OWN) do
        if o[5] == "wm" and not hooks.in_wm then continue end
        local name = tr(o[1])
        local hit = name:lower():find(q, 1, true) ~= nil
        if not hit then
            for w in string.gmatch(o[2], "%S+") do
                if w:sub(1, #q) == q then hit = true; break end
            end
        end
        if hit and #found < 3 then
            found[#found + 1] = { name = name, kind_label = "Marea", icon = "", glyph = o[3], own = k }
        end
    end
    return found
end

local function search()
    local q = (text.query or ""):lower()
    generation = generation + 1
    local mine = generation
    local matches = {}
    for _, a in ipairs(apps) do
        if q == "" or a.name:lower():find(q, 1, true) then matches[#matches + 1] = a end
    end
    table.sort(matches, prefer)
    local list = own_matches(q)
    --  Then the windows already open whose program or title it names: going
    --  to one is quicker than opening another.
    if #q >= 2 then
        local shown = 0
        for _, w in ipairs(open_windows) do
            local class, title = tostring(w.class or ""), tostring(w.title or "")
            if shown < 2 and (class:lower():find(q, 1, true) or title:lower():find(q, 1, true)) then
                shown = shown + 1
                list[#list + 1] = { name = title ~= "" and title or class, kind_label = tr("Window") .. (class ~= "" and " · " .. class or ""), icon = class, window_id = w.id }
            end
        end
    end
    for _, a in ipairs(matches) do
        if #list < 7 then
            list[#list + 1] = { name = a.name, kind_label = tr("Application"), icon = a.icon, exec = a.exec }
        end
    end
    --  With no query, «at hand»: what the system gives first.
    if q == "" then paint(q, list); return end
    paint(q, list)
    --  And the files and folders, which arrive when they arrive. With fewer
    --  than three letters there is no search: `fd` with one letter returns
    --  half your home —thousands of paths that neither fit nor say anything—
    --  and all it achieves is making the list jump with every key.
    if #q < 3 then return end
    run("fd", { "--max-depth", "4", "--max-results", "12", "--hidden",
                "--exclude", ".git", "--exclude", "node_modules", "--exclude", ".cache",
                "--color", "never", q, home_dir }, function(output)
        if mine ~= generation then return end
        for line in tostring(output):gmatch("[^\n]+") do
            if #list < 7 then
                local is_folder = line:sub(-1) == "/"
                local clean = is_folder and line:sub(1, -2) or line
                local name = clean:match("([^/]+)$") or clean
                local where = clean:sub(#home_dir + 2, #clean - #name - 1)
                list[#list + 1] = {
                    name = name,
                    kind_label = (is_folder and tr("Folder") or tr("File")) .. (where ~= "" and " · " .. where or ""),
                    icon = is_folder and "folder" or "text-x-generic",
                    filepath = clean,
                    path = clean,
                }
            end
        end
        if mine == generation then paint(q, list) end
    end)
end

if not sys.watch("apps", function(list) apps = list; search() end) then
    log("this system does not know which applications it has")
end
on("text:query", search)
on("fact:searching", function(opened) if opened then search() end end)

--  Opening: an app is launched; a file or a folder, to whoever knows how to open it.
on("open_result", function(i)
    local h = model.results[i + 1]
    if h == nil then return end
    if h.own then
        log("opening, of hers:", h.name)
        text.query = ""
        --  The finder is closing as this arrives; the card opens a moment
        --  later, so the two do not cross.
        after(180, OWN[h.own][4])
        return
    end
    if h.window_id then
        log("going to the window", h.name)
        window_call("activate", h.window_id)
    elseif h.exec then
        log("opening", h.name)
        sys.call("apps.launch", h.exec)
    else
        --  To whoever knows how to open it: the desktop decides that, not this bar.
        log("opening", h.filepath or h.name)
        if h.filepath then run("xdg-open", { h.filepath }, function() end) end
    end
    text.query = ""
end)

-- ── her menu ───────────────────────────────────────────────────────
--  What the right button offers. The two rows of the house are put by the
--  scene; this brings the plugins' ones, which is the only thing that changes
--  from one machine to another. There is no catalogue here, so they are
--  spelled out: what matters is that the border is written down and that
--  grouping is this side's business.
--
--  `first` says whether it opens a group, and then the row takes 16 px more
--  for the name of whoever brings it. It is a count and not a coordinate:
--  where each row falls is shared out by the scene, which has a column for
--  exactly that.
--  Their visible names go through `tr()` each time the menu is set, so that a
--  change of language reaches them too (see `fact:locale` at the end).
local plugin_entries = {
    { id = "desk.walls",    group = "Desktop", title = "Wallpapers" },
    { id = "drift.open",    group = "Drift", title = "Her library" },
    { id = "drift.paste",   group = "Drift", title = "Keep what is copied" },
    { id = "swell.history", group = "Swell", title = "History" },
}

--  In pleamar-wm's own session its window manager answers to `wm`: her menu
--  and her finder can switch it between tiled and free windows, and bring
--  back every window put away.
local in_wm = false
do
    local ok, where = pcall(sys.ask, "env", "PLEAMAR_SOCKETS")
    in_wm = ok and type(where) == "string" and where:find("pleamar%-pleamar") ~= nil
end
local function wm_toggle_free()
    run("pleamar-wm", { "--say", "wm", "emit toggle_free" }, nil, { output = false })
end
--  Rain on the windows, as if they were glass: the window manager's shader.
--  While it rains, her menu has one more entry: how hard, from a drizzle to a
--  downpour, a press going round the three.
--  While it rains, her control center has a fourth level: how hard, from a
--  drizzle to a downpour. Dragging it sends the window manager a new amount
--  every few percent, not every pixel: each one is a process.
local raining = false
local rain_amount = 1
local rain_sent = -1
local function wm_rain_level_send(force)
    if not force and math.abs(rain_amount - rain_sent) < 0.04 and rain_amount > 0.1 and rain_amount < 1 then return end
    rain_sent = rain_amount
    run("pleamar-wm", { "--say", "wm", "emit rain_level " .. string.format("%.2f", rain_amount) }, nil, { output = false })
end
local function refresh_rain()
    fact.rain_on = in_wm and raining
    fact.rain_amount = rain_amount
end
local function wm_toggle_rain()
    run("pleamar-wm", { "--say", "wm", "emit toggle_rain" }, nil, { output = false })
    raining = not raining
    if raining then wm_rain_level_send(true) end
    refresh_rain()
end
on("set_rain", function(v)
    rain_amount = math.max(0.1, math.min(1, v))
    fact.rain_amount = rain_amount
    wm_rain_level_send(false)
end)
--  From her finder: it rains (if it was not already), and her control center
--  opens with the level to drag.
local function wm_rain_level()
    if not raining then wm_toggle_rain() end
    fact.open = true
    fact.page = "none"
end
--  Or snow: frost on the glass and snow piled on top (the rain stops).
local function wm_toggle_snow()
    run("pleamar-wm", { "--say", "wm", "emit toggle_snow" }, nil, { output = false })
    raining = false
    refresh_rain()
end
--  She rides the window with the keyboard: she leaves by the top edge and the
--  window manager draws her on its title bar (see `ride` in its scene). Asked
--  again while she is out, she comes home.
local riding = false
local function wm_ride()
    if riding then
        run("pleamar-wm", { "--say", "wm", "emit unride" }, nil, { output = false })
        return
    end
    riding = true
    emit("ride_out")
end
on("gone", function()
    if fact.purpose ~= "ride" then return end
    run("pleamar-wm", { "--say", "wm", "emit ride" }, nil, { output = false })
end)
on("ride_back", function() riding = false end)
local function wm_restore_all()
    if hooks.restore_all then hooks.restore_all() end
end
hooks.in_wm = in_wm
hooks.wm_free = wm_toggle_free
hooks.wm_restore = wm_restore_all
hooks.wm_rain = wm_toggle_rain
hooks.wm_rain_level = wm_rain_level
hooks.wm_snow = wm_toggle_snow
hooks.wm_ride = wm_ride
if in_wm then
    table.insert(plugin_entries, 1, { id = "wm.free", group = "Windows", title = "Tiled or free windows" })
    table.insert(plugin_entries, 2, { id = "wm.restore", group = "Windows", title = "Bring back every window" })
    table.insert(plugin_entries, 3, { id = "wm.rain", group = "Windows", title = "Rain on the windows" })
    table.insert(plugin_entries, 4, { id = "wm.snow", group = "Windows", title = "Snow on the windows" })
    table.insert(plugin_entries, 5, { id = "wm.ride", group = "Windows", title = "Ride a window" })
end

local function set_menu()
    local group, groups = nil, 0
    local entries = {}
    for _, e in ipairs(plugin_entries) do
        e.first = e.group ~= group
        if e.first then
            group = e.group
            groups = groups + 1
        end
        entries[#entries + 1] = { id = e.id, group_name = tr(e.group), title = tr(e.title), first = e.first }
    end
    fact.groups = groups
    model.entries = entries
end
set_menu()

on("menu_entry", function(i)
    local e = plugin_entries[i + 1]
    if e ~= nil and e.id == "wm.free" then wm_toggle_free(); return end
    if e ~= nil and e.id == "wm.restore" then wm_restore_all(); return end
    if e ~= nil and e.id == "wm.rain" then wm_toggle_rain(); return end
    if e ~= nil and e.id == "wm.snow" then wm_toggle_snow(); return end
    if e ~= nil and e.id == "swell.history" then emit("show_history"); return end
    if e ~= nil and e.id == "drift.open" then emit("show_drift"); return end
    if e ~= nil and e.id == "desk.walls" then emit("show_walls"); return end
    if e ~= nil and e.id == "drift.paste" then if hooks.keep_copied then hooks.keep_copied() end; return end
    if e ~= nil and e.id == "wm.ride" then wm_ride(); return end
    log("a plugin entry from her menu", e ~= nil and e.id or i)
end)

-- ── the swell, in demo mode ────────────────────────────────────────
--  pleamar knows how to receive the desktop's real notifications: you ask for
--  the `notifications` permission and `sys.watch("notifications", …)` gives the
--  live list, with its application, its title, its body and its urgency. Here
--  that permission is NOT given on purpose: whoever holds that place on this
--  machine is Marea, and the first one to ask for it keeps it. A prototype does
--  not take the notices away from the real bar.
--
--  So this is the other thing: an invented batch that comes in on its own, to
--  be able to see the whole swell —the arrival, the burst, the tray and the
--  urgent one— without waiting for someone to write to you. What is sent
--  across the border is exactly what the service would send: `cat`, `who`,
--  `line` and the event. The day the permission is given, the only thing that
--  changes is where they come from.
--
--  It is stopped with `pleamar --say marea "fact demo false"`.

--  1 messages · 2 work · 3 calendar · 4 system. It is what decides the colour
--  of the drop, so a notice with no known category is «system».
local CATEGORY_OF = { TELEGRAM = 1, MAIL = 2, CALENDAR = 3, SYSTEM = 4 }

--  Written in English and shown through `tr()` when it arrives: they are
--  Marea's own words, not someone else's notification. A real one has an `id`;
--  an invented one does not, and that is how `shown()` tells them apart.
local batch = {
    { app = "TELEGRAM", title = "Ana Ruiz",            detail = "Lunch today? I have a table at two", delay = 1400 },
    { app = "MAIL",     title = "September invoice",   detail = "due on the 30th",                    delay = 5200 },
    --  Two in a row: the burst stacks up and «+N more» shows instead of three jumps.
    { app = "TELEGRAM", title = "Building site group", detail = "6 unread messages",                  delay = 8600 },
    { app = "TELEGRAM", title = "Marcos",              detail = "I'll call you in five",              delay = 9100 },
    { app = "CALENDAR", title = "Design review",       detail = "in 10 minutes · big room",           delay = 13000 },
    --  And an urgent one, which is not grouped: its own row, on top and in amber.
    { app = "SYSTEM",   title = "Battery at 8 %",      detail = "about 12 minutes left", urgent = true, delay = 18000 },
}
--  What each row keeps saying about when it arrived. There is no clock here:
--  the place in the list already tells the story, and it is what the contract
--  does.
local AGES = { "now", "2 min", "9 min", "26 min", "1 h" }

local rows = {}

--  What a row shows: Marea's own words in the language of the moment; what
--  comes from another application, as it came.
local function shown(f, s)
    if f.id == nil then return tr(s) end
    return s
end

local function render()
    for k = 1, 5 do
        local f = rows[k]
        fact["alive." .. k] = f ~= nil
        if f ~= nil then
            text["row." .. k .. ".app"] = shown(f, f.app)
            text["row." .. k .. ".title"] = shown(f, f.title)
            text["row." .. k .. ".detail"] = shown(f, f.detail)
            text["row." .. k .. ".age"] = tr(AGES[k])
            fact["row_cat." .. k] = f.cat
            fact["row_urgent." .. k] = f.urgent == true
        end
    end
end

--  One arrives: first it is said WHAT it is —the scene needs it before the
--  event, because the card that flies already carries its name written on it—
--  and then that it has arrived. An urgent one, besides, is announced as such.
--  What a real notice is about, by who sends it. It is what decides the colour
--  of its drop, so whatever is not recognised is «system» and nothing is made up.
local CATEGORIES = {
    { 1, { "telegram", "whatsapp", "signal", "discord", "element", "fractal", "mensaj", "chat" } },
    { 2, { "thunderbird", "mail", "correo", "geary", "evolution", "outlook", "slack", "teams", "github" } },
    { 3, { "calendar", "calendario", "khal", "reminder", "recordatorio", "agenda" } },
}
local function classify(app)
    local a = string.lower(app or "")
    for _, c in ipairs(CATEGORIES) do
        for _, fragment in ipairs(c[2]) do
            if string.find(a, fragment, 1, true) then return c[1] end
        end
    end
    return 4
end

--  The actions of the notice that arrives, for its chips: its own ones, not
--  the default (that is «Open»), two at most.
local arriving_actions = {}

--  What the arriving card shows besides the words: its icon, whether it is
--  urgent, and what can be done with it without opening anything. Called for
--  the one that arrives, and again for the next one of the pile when the one
--  in front is done.
local function show_front(n)
    text.who = shown(n, n.title)
    text.line = shown(n, n.detail)
    model.notice_face = { { icon = n.icon or "" } }
    fact["notice.icon"] = (n.icon or "") ~= ""
    fact.reading_urgent = n.urgent == true
    arriving_actions = {}
    for _, a in ipairs(n.actions or {}) do
        if a.key ~= "default" and #arriving_actions < 2 then arriving_actions[#arriving_actions + 1] = a end
    end
    text["act.1"] = arriving_actions[1] and arriving_actions[1].label or ""
    text["act.2"] = arriving_actions[2] and arriving_actions[2].label or ""
    fact.acts = #arriving_actions
end

--  Whether a notice comes in quietly —straight to its drop, no card—: with
--  the screen locked, while a recording is going on (unless that is turned
--  off), and with «do not disturb», which an urgent one gets through.
local function quiet_for(n)
    if fact.locked == true then return true end
    if (fact.taping == true or fact.sharing == true) and fact.quiet_rec ~= false then return true end
    return fact.dnd == true and not n.urgent
end

local function arrive(n)
    n.cat = CATEGORY_OF[n.app] or classify(n.app)
    table.insert(rows, 1, n)
    while #rows > 5 do table.remove(rows) end
    render()
    fact.cat = n.cat
    show_front(n)
    fact.hush = quiet_for(n)
    emit("notice")
    if n.urgent then emit("urge") end
end

--  The whole batch, and again after 22 s. Before repeating, the tray and the
--  drops' counts are emptied: otherwise, the second round starts with the
--  first one still on top and nothing can be seen any more.
local function clear()
    rows = {}
    render()
    for k = 1, 4 do
        fact["n." .. k] = 0
        fact["last." .. k] = 0
    end
end

--  One batch and stop. In a loop it was a nuisance: what you want is to see it
--  once and be able to repeat it when it suits you, not have it rain on you
--  while you work. It is repeated with `pleamar --say marea "emit another_batch"`,
--  which empties whatever there was and starts again.
local function playBatch()
    clear()
    for _, n in ipairs(batch) do
        after(n.delay, function() if fact.demo ~= false then arrive(n) end end)
    end
end

on("another_batch", playBatch)

-- ── the real notices ───────────────────────────────────────────────
--  The service gives the whole live list every time it changes, the newest
--  first. Three things have to be got out of it: the ones that are new —which
--  come in through the same `arrive()` as the demo, because it is the same
--  swell—, the ones that have changed while still being the same one —a
--  download that moves forward— and the ones that are no longer there without
--  us having removed them: their application closed them, which is what
--  happens when you read the message in Telegram.
-- ── what has washed up: the history ───────────────────────────────
--  Every real notice once it is dealt with —done, thrown away, opened, or
--  closed by its own application— goes here, newest first, and to disk
--  (`history.json` in her folder), so a restart does not wash it away. The
--  page shows a window of it, a row for each day and the notices under it.
local history = {}
do
    local ok, loaded = pcall(sys.ask, "files.read", "history.json", "json")
    if ok and type(loaded) == "table" and type(loaded.items) == "table" then history = loaded.items end
end
local history_offset = 0
local history_saving = false
local function save_history()
    if history_saving then return end
    history_saving = true
    after(1000, function()
        history_saving = false
        pcall(sys.call, "files.write", "history.json", { items = history })
    end)
end
local DAY_NAMES = { "Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday" }
local function day_label(t)
    local now = os.date("*t")
    local midnight = os.time({ year = now.year, month = now.month, day = now.day, hour = 0 })
    if t >= midnight then return tr("Today") end
    if t >= midnight - 86400 then return tr("Yesterday") end
    local d = os.date("*t", t)
    return string.format("%s %d", tr(DAY_NAMES[d.wday]), d.day)
end
local function paint_history()
    local rows, last_day = {}, nil
    for k = history_offset + 1, #history do
        local h = history[k]
        local day = day_label(h.time or 0)
        if day ~= last_day then
            if #rows >= 8 then break end
            rows[#rows + 1] = { head = true, day = day, app = "", title = "", detail = "", when = "", cat = 4, age = 0 }
            last_day = day
        end
        if #rows >= 9 then break end
        rows[#rows + 1] = { head = false, day = "", app = h.app or "", title = h.title or "", detail = h.detail or "",
                            when = os.date("%H:%M", h.time or 0), cat = h.cat or 4,
                            age = math.min(1, (os.time() - (h.time or 0)) / 604800) }
    end
    model.past = rows
end
local function remember(f)
    if f == nil or f.id == nil or f.remembered then return end
    f.remembered = true
    table.insert(history, 1, { app = f.app, title = f.title, detail = f.detail, cat = f.cat, time = os.time() })
    while #history > 300 do table.remove(history) end
    save_history()
    if fact.page == "history" then paint_history() end
end
on("history_open", function()
    history_offset = 0
    paint_history()
end)
--  The wheel: up, newer; down, older. Three at a time.
on("history_scroll", function(w)
    local step = (w or 0) > 0 and -3 or 3
    history_offset = math.max(0, math.min(history_offset + step, math.max(#history - 1, 0)))
    paint_history()
end)
on("clear_history", function()
    history = {}
    history_offset = 0
    save_history()
    paint_history()
end)

-- ── Deriva: what is dropped on her, kept, and found again ──────────
--  Her library is `deriva-worker`, the same one the Quickshell Marea used: a
--  local SQLite with the files kept by their hash, which saves, searches and
--  gives back. It answers a line of JSON. What is let go on her goes in —the
--  inbox, or «Read later» / «Inspiration» if it was let go on their drops—, and
--  the page shows it again as cards.
local DRIFT = "deriva-worker"
local drift_items, drift_offset, drift_blobs = {}, 0, nil

local function drift_run(args, done)
    run(DRIFT, args, function(out, code)
        local ok, v = pcall(json.decode, tostring(out or ""))
        if code ~= 0 or not ok or type(v) ~= "table" then
            log("deriva ·", args[1], "did not answer", code)
            if done then done(nil) end
            return
        end
        --  Its answers come wrapped (`{ ok, result }`); `where` does not.
        if v.ok == false then
            log("deriva ·", args[1], "said no:", v.error and v.error.message or "?")
            if done then done(nil) end
            return
        end
        if done then done(v.result or v) end
    end)
end
--  Where it keeps the files. No answer at all: the library is not there, or
--  not yet (her launcher builds it the first time, in the background).
local function drift_where(done)
    drift_run({ "where" }, function(r)
        if r then drift_blobs = r.blobs end
        fact["drift.missing"] = r == nil
        if done then done() end
    end)
end
drift_where()

local function host_of(u) return tostring(u or ""):match("^%a+://([^/]+)") or "" end
local function blob_path(it)
    local h = it.blob_hash
    if drift_blobs == nil or type(h) ~= "string" or #h < 4 then return "" end
    return drift_blobs .. "/" .. h:sub(1, 2) .. "/" .. h:sub(3, 4) .. "/" .. h
end
local function link_of(it)
    if type(it.source_url) == "string" and it.source_url ~= "" then return it.source_url end
    return blob_path(it)
end

local function show_drift()
    local cards = {}
    for k = drift_offset + 1, math.min(drift_offset + 4, #drift_items) do
        local it = drift_items[k]
        local title = (it.title ~= nil and it.title ~= "") and it.title or it.blob_name or it.excerpt or "…"
        local source = (it.source ~= nil and it.source ~= "" and it.source ~= "web") and it.source or host_of(it.source_url)
        local preview = type(it.preview_path) == "string" and it.preview_path or ""
        cards[#cards + 1] = { title = tostring(title):gsub("%s+", " "), source = source, kind = string.upper(tostring(it.type or "")),
                              date = day_label(math.floor((it.captured_at or 0) / 1000)), preview = preview,
                              has_preview = preview ~= "", link = link_of(it) }
    end
    model.drift = cards
end

local function drift_load()
    local q = text.drift_q or ""
    local args = q == "" and { "list", "--limit", "60" } or { "search", "--query", q, "--limit", "60" }
    drift_run(args, function(r)
        drift_items = (r and r.items) or {}
        drift_offset = 0
        if q == "" then fact["drift.kept"] = #drift_items end
        show_drift()
    end)
end
on("drift_open", function()
    if fact["drift.missing"] then drift_where(drift_load) else drift_load() end
end)
--  Typing searches, a moment after the last key.
local drift_typed = 0
on("text:drift_q", function()
    drift_typed = drift_typed + 1
    local mine = drift_typed
    after(260, function() if mine == drift_typed then drift_load() end end)
end)
on("drift_scroll", function(w)
    local step = (w or 0) > 0 and -2 or 2
    drift_offset = math.max(0, math.min(drift_offset + step, math.max(#drift_items - 2, 0)))
    drift_offset = drift_offset - drift_offset % 2
    show_drift()
end)
on("drift_pick", function(i)
    local it = drift_items[drift_offset + i + 1]
    if it == nil then return end
    local link = link_of(it)
    if link ~= "" then run("xdg-open", { link }, function() end) end
end)

--  What a file is, by its extension: it decides its icon and its filters.
local KINDS = {
    image = { "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "avif", "heic" },
    video = { "mp4", "mkv", "webm", "mov", "avi" },
    audio = { "mp3", "ogg", "flac", "wav", "m4a", "opus" },
    document = { "pdf", "odt", "doc", "docx", "txt", "md", "epub", "rtf", "ods", "xlsx", "odp", "pptx" },
    code = { "rs", "py", "js", "ts", "lua", "luau", "c", "h", "cpp", "go", "java", "sh", "json", "toml", "yaml", "yml", "plm", "qml", "html", "css" },
}
local function kind_of(path)
    local ext = tostring(path):match("%.([%w]+)$")
    ext = ext and ext:lower() or ""
    for kind, list in pairs(KINDS) do
        for _, e in ipairs(list) do if e == ext then return kind end end
    end
    return "file"
end
local function url_decode(s) return (s:gsub("%%(%x%x)", function(h) return string.char(tonumber(h, 16)) end)) end

local SPACES = { drift_drop = nil, drift_later = "leer-luego", drift_insp = "inspiracion" }
local PLACES = { drift_drop = "Deriva", drift_later = "Read later", drift_insp = "Inspiration" }

local function keep(data, mime, zone)
    local urls, groups, words = {}, {}, {}
    for line in tostring(data or ""):gmatch("[^\r\n]+") do
        local l = line:match("^%s*(.-)%s*$")
        if l:match("^file://") then
            local p = url_decode(l:gsub("^file://[^/]*", ""))
            local k = kind_of(p)
            groups[k] = groups[k] or {}
            table.insert(groups[k], p)
        elseif l:match("^https?://%S+$") then
            urls[#urls + 1] = l
        elseif l ~= "" and not l:match("^#") then
            words[#words + 1] = l
        end
    end
    local requests = {}
    for _, u in ipairs(urls) do requests[#requests + 1] = { type = "url", source_url = u } end
    for k, paths in pairs(groups) do requests[#requests + 1] = { type = k, paths = paths } end
    if #words > 0 and mime ~= "text/uri-list" then requests[#requests + 1] = { type = "text", text = table.concat(words, "\n") } end
    if #requests == 0 then return end
    local space = SPACES[zone]
    local saved, duplicates, failed, left = 0, 0, 0, #requests
    local function finished()
        if saved > 0 then
            fact["drift.result"] = 1
            text["drift.toast"] = string.format(tr("Kept in %s"), tr(PLACES[zone] or "Deriva"))
        elseif duplicates > 0 then
            fact["drift.result"] = 2
            text["drift.toast"] = tr("It was already there")
        else
            fact["drift.result"] = 3
            text["drift.toast"] = tr("Could not keep it")
        end
        emit("drift_saved")
        if fact.page == "drift" then drift_load() end
    end
    for _, req in ipairs(requests) do
        if space then req.space = space end
        drift_run({ "ingest", "--request", json.encode(req) }, function(r)
            if r then
                saved = saved + (r.saved or 0)
                duplicates = duplicates + (r.duplicates or 0)
            else
                failed = failed + 1
            end
            left = left - 1
            if left == 0 then finished() end
        end)
    end
end
for zone in pairs(PLACES) do
    on("drop:" .. zone, function(data, mime) keep(data, mime, zone) end)
end
--  And what is copied, from her menu or the finder: kept without carrying it.
local function keep_copied()
    local ok, copied = pcall(sys.ask, "clipboard")
    if ok and type(copied) == "string" and copied ~= "" then keep(copied, "text/plain", "drift_drop") end
end
hooks.keep_copied = keep_copied

local seen = {}
--  The ones already dealt with here, while the service has not yet said they
--  are gone: they must not come back to fill a gap.
local finished = {}

local function remove_done()
    for k = #rows, 1, -1 do
        if rows[k].done then
            if rows[k].id then finished[rows[k].id] = true end
            table.remove(rows, k)
        end
    end
end

--  And no drop can count more than there are left of its kind: whatever
--  path removed one —resolved, dismissed, closed by its application—, the
--  number beside her comes down to what is really pending. Only down: a drop
--  flying back is counted by the scene when it lands. Counted from the
--  service's list, not from the rows: the tray keeps five and there may be
--  more. A resolved one still counts while its drop flies home; a dismissed
--  one no. With made-up notices there is no list, and nothing is capped.
local live = nil
--  The ones sent «later»: out of sight and out of the counts until their
--  time comes. And the apps muted: their notices go straight to the history.
local snoozed = {}
local muted_apps = {}
local function cap_counts()
    if live == nil then return end
    local gone = {}
    for _, f in ipairs(rows) do
        if f.id and f.done and not f.flying then gone[f.id] = true end
    end
    for id in pairs(snoozed) do gone[id] = true end
    local left = { 0, 0, 0, 0 }
    for _, n in ipairs(live) do
        if not gone[n.id] then
            local app = string.upper(n.app ~= "" and n.app or tr("notice"))
            local c = CATEGORY_OF[app] or classify(app)
            if left[c] then left[c] = left[c] + 1 end
        end
    end
    for k = 1, 4 do
        local c = "n." .. k
        if (fact[c] or 0) > left[k] then fact[c] = left[k] end
    end
end
--  The scene counts on its own when a drop sinks in, which can be seconds
--  after the notice came —she was asleep and the drop had to fall—: by then
--  it may already be closed.
for k = 1, 4 do on("fact:n." .. k, cap_counts) end

--  A row made from one of the service's notices, without arriving again.
local function row_of(n)
    local app = string.upper(n.app ~= "" and n.app or tr("notice"))
    return { id = n.id, app = app, title = n.title, detail = n.body, urgent = (n.urgency or 1) >= 2,
             actions = n.actions, icon = n.icon, cat = CATEGORY_OF[app] or classify(app) }
end

--  What hovering over her shows: the newest one still pending. Not while a
--  card is being read: that one is using the same words.
local function refresh_preview()
    if (fact.phase or 0) ~= 0 then return end
    for _, f in ipairs(rows) do
        if not f.done then
            show_front(f)
            fact.cat = f.cat
            return
        end
    end
end

--  The tray shows five, and the service may keep more: the ones that did not
--  fit waited there, counted beside her, and with no row they could never be
--  dealt with. Whatever room there is gets filled from the service's list,
--  newest first. Only with the tray closed: rows do not move under the pointer.
local function refill()
    if live == nil or fact.tray_open then return end
    --  A drop still flying home counts by its row: that row stays where it is.
    for _, f in ipairs(rows) do
        if f.flying then return end
    end
    remove_done()
    local inrow = {}
    for _, f in ipairs(rows) do
        if f.id then inrow[f.id] = true end
    end
    for _, n in ipairs(live) do
        if #rows >= 5 then break end
        if not inrow[n.id] and not finished[n.id] and not snoozed[n.id] then
            rows[#rows + 1] = row_of(n)
        end
    end
    table.sort(rows, function(a, b) return (a.id or 0) > (b.id or 0) end)
    render()
    refresh_preview()
end
on("fact:tray_open", refill)

--  In pleamar-wm, the shore shows what is unread on each program's icon: it
--  is told how many notices each one has, whenever that changes.
local sent_badges = nil
local function tell_badges(list)
    if not in_wm then return end
    local counts, order = {}, {}
    for _, n in ipairs(list or {}) do
        local a = (tostring(n.app or ""):lower():gsub("%s+", "_"))
        if a ~= "" then
            if counts[a] == nil then order[#order + 1] = a end
            counts[a] = (counts[a] or 0) + 1
        end
    end
    local parts = {}
    for _, a in ipairs(order) do parts[#parts + 1] = a .. ":" .. counts[a] end
    local text = table.concat(parts, " ")
    if text == sent_badges then return end
    sent_badges = text
    run("pleamar-wm", { "--say", "wm", "text win.badges " .. text }, nil, { output = false })
end

local function from_service(list)
    local present = {}
    for _, n in ipairs(list) do present[n.id] = n end
    --  What the service no longer has needs no remembering.
    for id in pairs(finished) do
        if present[id] == nil then finished[id] = nil end
    end
    local changed = false
    for k = #rows, 1, -1 do
        local f = rows[k]
        if f.id and not f.done then
            local n = present[f.id]
            if n == nil then
                --  Whoever sent it closed it: the row goes and its drop comes down.
                remember(f)
                local c = "n." .. f.cat
                if (fact[c] or 0) > 0 then fact[c] = fact[c] - 1 end
                table.remove(rows, k)
                changed = true
            elseif n.title ~= f.title or n.body ~= f.detail then
                f.title, f.detail = n.title, n.body
                changed = true
            end
        end
    end
    if changed then render() end
    live = list
    cap_counts()
    for i = #list, 1, -1 do
        local n = list[i]
        if not seen[n.id] and muted_apps[string.upper(n.app ~= "" and n.app or tr("notice"))] then
            --  A muted app: kept in the history, and gone.
            seen[n.id] = true
            finished[n.id] = true
            remember(row_of(n))
            sys.call("notifications.dismiss", n.id)
        elseif not seen[n.id] then
            seen[n.id] = true
            --  The resolved ones waited in their place so as not to move the
            --  rows mid-animation; when a new one comes in everything is
            --  rearranged, and they go.
            remove_done()
            arrive({ id = n.id, app = string.upper(n.app ~= "" and n.app or tr("notice")), title = n.title, detail = n.body,
                     urgent = (n.urgency or 1) >= 2, actions = n.actions, icon = n.icon })
        end
    end
    refill()
    tell_badges(live)
end

if sys.watch("notifications", from_service) then
    --  This is a notice CENTRE, not a bar of bubbles: «everything arrives, you
    --  decide». If they expired on their own after six seconds, their
    --  application would stop listening and they could no longer be opened
    --  from here.
    sys.call("notifications.keep", true)
    fact.demo = false
    log("notifications · the real ones: this is who receives them now")
else
    log("demo · the notices are made up: pleamar gives up the place to whoever already has it")
    log("demo · another batch: pleamar --say marea \"emit another_batch\"")
    after(600, playBatch)
end

-- ── the agents' reservoirs ────────────────────────────────────────
--  What is left of Claude Code's and Codex's windows. `tools/reservas` reads
--  them —Claude from its server with the token Claude Code already keeps,
--  Codex from its own session logs— and prints one line per thing. Here they
--  go into the scene's facts; the countdowns are kept up to date every half
--  minute without asking anybody again. The rules of the old Marea hold: an
--  unknown number is not a zero, the percentages of two windows are never
--  added up, and a window past its renewal is «pending», not full.
local agents = {}
local agents_now = 0

local function span(seconds)
    seconds = math.max(0, math.floor(seconds))
    local d, h, m = seconds // 86400, (seconds % 86400) // 3600, (seconds % 3600) // 60
    if d > 0 then return string.format("%d d %d h", d, h) end
    if h > 0 then return string.format("%d h %02d", h, m) end
    return string.format("%d min", math.max(m, 1))
end

local WINDOW_NAMES = { [300] = "5 hours", [10080] = "Weekly" }

local function paint_agents()
    local now = os.time()
    local worst = 101
    for a = 1, 2 do
        local g = agents[a]
        fact["ag.kind." .. a] = g and (g.id == "claude" and 1 or (g.id == "codex" and 2 or 3)) or 0
        text["ag.name." .. a] = g and g.name or ""
        if g then
            local parts = {}
            if g.plan ~= "" then table.insert(parts, g.plan) end
            if g.origin ~= "" then table.insert(parts, tr(g.origin == "servidor" and "server" or "on this machine")) end
            if g.observed then
                local ago = now - g.observed
                table.insert(parts, ago < 90 and tr("just now") or string.format(tr("%s ago"), span(ago)))
            end
            text["ag.sub." .. a] = table.concat(parts, " · ")
        else
            text["ag.sub." .. a] = ""
        end
        --  A reading older than a quarter of an hour is drawn, never trusted
        --  for her face or the little tube.
        local fresh = g and g.observed and now - g.observed < 900
        for l = 1, 3 do
            local w = g and g.limits[l]
            local k = a .. "." .. l
            fact["ag.on." .. k] = w ~= nil
            if w then
                fact["ag.active." .. k] = w.active
                local pending = w.renews and w.renews <= now
                fact["ag.pending." .. k] = pending or false
                fact["ag.used." .. k] = w.used or -1
                text["ag.lname." .. k] = w.minutes and WINDOW_NAMES[w.minutes] and tr(WINDOW_NAMES[w.minutes]) or w.name
                if pending then
                    text["ag.lrenew." .. k] = tr("renewed?")
                elseif not w.active then
                    text["ag.lrenew." .. k] = tr("not started")
                elseif w.renews then
                    text["ag.lrenew." .. k] = string.format(tr("in %s"), span(w.renews - now))
                else
                    text["ag.lrenew." .. k] = ""
                end
                if w.renews and w.minutes then
                    fact["ag.elapsed." .. k] = math.clamp(1 - (w.renews - now) / (w.minutes * 60), 0, 1)
                else
                    fact["ag.elapsed." .. k] = 0
                end
                if fresh and w.active and w.used and not pending then worst = math.min(worst, 100 - w.used) end
            else
                fact["ag.used." .. k] = -1
                text["ag.lname." .. k] = ""
                text["ag.lrenew." .. k] = ""
            end
        end
    end
    fact["ag.worst"] = worst
end

local reading_agents = false
local function read_agents()
    if reading_agents then return end
    reading_agents = true
    run("node", { "tools/reservas", "--lines" }, function(out, code)
        reading_agents = false
        if code ~= 0 or out == nil then log("agents · could not read the windows (" .. tostring(code) .. ")"); return end
        local list, by_id = {}, {}
        for line in string.gmatch(out, "[^\n]+") do
            local f = string.split(line, "\t")
            local num = function(i) return tonumber(f[i]) end
            if f[1] == "agent" and #list < 2 then
                local g = { id = f[2], name = f[3] or "", plan = f[4] or "", origin = f[5] or "", observed = num(6), limits = {} }
                table.insert(list, g)
                by_id[g.id] = g
            elseif f[1] == "limit" and by_id[f[2]] and #by_id[f[2]].limits < 3 then
                local used = num(5)
                if used and (used < 0 or used > 100) then used = nil end
                table.insert(by_id[f[2]].limits, { name = f[3] or "", minutes = num(4), used = used,
                    renews = num(6), active = f[7] == "1", pending = f[8] == "1" })
            end
        end
        agents = list
        paint_agents()
        emit("agents_read")
    end)
end

on("agents_refresh", read_agents)
--  Every five minutes in the background, for the little tube by the gear;
--  the countdowns, every half minute, without asking anybody.
after(4000, read_agents)
every(300000, read_agents)
every(30000, paint_agents)

-- ── glances ─────────────────────────────────────────────────────
--  Every 7 to 16 s she looks for a moment at where the mouse is, wherever it
--  is. The scene does the looking; here it is only decided when.
local function glance_later()
    after(math.random(7000, 16000), function()
        fact.glancing = true
        after(math.random(1200, 2200), function()
            fact.glancing = false
            glance_later()
        end)
    end)
end
glance_later()

-- ── the swell ──────────────────────────────────────────────────────
--  Resolving a row is marking it read wherever it really lives. Here it is
--  only noted down: what is seen —the row leaving, the drop going back, the
--  count going down— the scene has already done, without waiting for this.
--  Resolved is not closed yet: the scene gives five seconds to change your
--  mind, and a closed notification does not come back. It is really closed
--  once the fuse has gone out, if nobody undid it.
on("resolve", function(k)
    local f = rows[k]
    if f == nil then return end
    f.done = true
    f.flying = true
    after(1000, function() f.flying = nil; refill() end)
    if f.id then
        after(5400, function()
            if f.done then
                remember(f)
                sys.call("notifications.dismiss", f.id)
            end
        end)
    end
end)
on("undone", function(k)
    local f = rows[k]
    if f then f.done = false end
end)
--  What is dismissed does not come back: no drop, no undo.
on("dismiss", function(k)
    local f = rows[k]
    if f == nil then return end
    f.done = true
    remember(f)
    if f.id then sys.call("notifications.dismiss", f.id) end
end)
--  A tap opens it where it lives: its default action, if it brings one. If it
--  brings none, tapping it can do no more than mark it as seen.
local function open_row(k)
    local f = rows[k]
    if f == nil or f.id == nil then return end
    local can_open = false
    for _, a in ipairs(f.actions or {}) do can_open = can_open or a.key == "default" end
    remember(f)
    if can_open then sys.call("notifications.invoke", f.id, "default") else sys.call("notifications.dismiss", f.id) end
end
on("open_notice", open_row)
--  The arriving card's chips: 0 open it, 1 done with it, 2 and 3 its own
--  actions, 9 done with the whole pile. They act on the one in front: the
--  newest of the pile that is not done yet.
local function front_row()
    local pile = math.max(1, math.floor(fact.burst or 1))
    for k = 1, math.min(pile, #rows) do
        if not rows[k].done then return k end
    end
    return 1
end
local function done_with(f)
    if f == nil or f.done then return end
    f.done = true
    remember(f)
    if f.id then sys.call("notifications.dismiss", f.id) end
end
on("notice_act", function(i)
    i = math.floor(i or 0)
    if i == 9 then
        local pile = math.max(1, math.floor(fact.burst or 1))
        for k = 1, math.min(pile, #rows) do done_with(rows[k]) end
        return
    end
    local k = front_row()
    local f = rows[k]
    if f == nil then return end
    if i == 10 or i == 11 then
        if i == 10 then hooks.snooze(f) else hooks.mute(f.app) end
        local next_k = front_row()
        if rows[next_k] and not rows[next_k].done then show_front(rows[next_k]) end
        return
    end
    if i == 0 then
        open_row(k)
    elseif i == 1 then
        done_with(f)
        --  And the next one of the pile comes to the front.
        local next_k = front_row()
        if rows[next_k] and not rows[next_k].done then show_front(rows[next_k]) end
    else
        local a = arriving_actions[i - 1]
        if a and f.id then sys.call("notifications.invoke", f.id, a.key) end
    end
end)
on("tray", function() log("the tray would open at the newest one") end)
-- ── the calendar ───────────────────────────────────────────────────
--  Her own: events kept in `calendar.json`, a month on screen (six weeks,
--  Monday first) and the day that is chosen. Writing «10:30 Dentist» makes an
--  event at that time; without a time it is for the whole day. When its time
--  comes, a notification goes out —with `notify-send`, to whoever receives
--  them, which on this machine is her— and it lands in the swell like any
--  other, as a calendar one.
local cal = { events = {}, year = 0, month = 0, sel = nil, rows = {} }
local MONTHS = { "January", "February", "March", "April", "May", "June", "July",
                 "August", "September", "October", "November", "December" }
local WEEKDAYS = { "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday" }
local LETTERS = { "Mo", "Tu", "We", "Th", "Fr", "Sa", "Su" }

local function days_in(y, m)
    local d = { 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31 }
    if m == 2 and (y % 4 == 0 and (y % 100 ~= 0 or y % 400 == 0)) then return 29 end
    return d[m]
end
--  0 Monday … 6 Sunday.
local function weekday(y, m, d)
    return (os.date("*t", os.time({ year = y, month = m, day = d, hour = 12 })).wday + 5) % 7
end
local function key(y, m, d) return string.format("%04d-%02d-%02d", y, m, d) end
local function today()
    local t = os.date("*t")
    return t.year, t.month, t.day
end
local function parse_key(k)
    local y, m, d = k:match("^(%d+)-(%d+)-(%d+)$")
    return tonumber(y), tonumber(m), tonumber(d)
end
local function on_day(k)
    local list = {}
    for _, e in ipairs(cal.events) do
        if e.date == k then list[#list + 1] = e end
    end
    --  The whole-day ones first, then by time.
    table.sort(list, function(a, b) return (a.time or "") < (b.time or "") end)
    return list
end

local function save_calendar()
    pcall(sys.call, "files.write", "calendar.json", { events = cal.events })
end
do
    local ok, loaded = pcall(sys.ask, "files.read", "calendar.json", "json")
    if ok and type(loaded) == "table" and type(loaded.events) == "table" then cal.events = loaded.events end
end

local function paint_calendar()
    local y, m = cal.year, cal.month
    text["cal.month"] = string.format(tr("%s %d"), tr(MONTHS[m]), y)
    for w = 1, 7 do text["cal.wd." .. w] = tr(LETTERS[w]) end
    --  The grid starts on the Monday of the week the 1st falls in.
    local py, pm = y, m - 1
    if pm < 1 then py, pm = y - 1, 12 end
    local lead = weekday(y, m, 1)
    local ty, tm, td = today()
    local d, cy_, cm = days_in(py, pm) - lead + 1, py, pm
    if lead == 0 then d, cy_, cm = 1, y, m end
    local counts = {}
    for _, e in ipairs(cal.events) do counts[e.date] = (counts[e.date] or 0) + 1 end
    for k = 0, 41 do
        local kk = key(cy_, cm, d)
        fact["cal.day." .. k] = d
        fact["cal.in." .. k] = cm == m
        fact["cal.today." .. k] = cy_ == ty and cm == tm and d == td
        fact["cal.n." .. k] = counts[kk] or 0
        if kk == cal.sel then fact["cal.sel"] = k end
        d = d + 1
        if d > days_in(cy_, cm) then
            d, cm = 1, cm + 1
            if cm > 12 then cm, cy_ = 1, cy_ + 1 end
        end
    end
    local sy, sm, sd = parse_key(cal.sel)
    text["cal.sel_day"] = tostring(sd)
    text["cal.sel_title"] = string.format("%s · %s", tr(WEEKDAYS[weekday(sy, sm, sd) + 1]), tr(MONTHS[sm]))
    local list = on_day(cal.sel)
    text["cal.sel_sub"] = #list == 0 and tr("Free day") or (#list == 1 and tr("1 event") or string.format(tr("%d events"), #list))
    local rows = {}
    cal.rows = {}
    for i, e in ipairs(list) do
        if i <= 6 then
            rows[#rows + 1] = { time = e.time ~= "" and e.time or tr("all day"), title = e.title }
            cal.rows[#cal.rows + 1] = e
        end
    end
    model.cal_events = rows
end

local function show_month(y, m, sel)
    cal.year, cal.month = y, m
    cal.sel = sel or key(y, m, 1)
    paint_calendar()
end

local function open_today()
    local y, m, d = today()
    show_month(y, m, key(y, m, d))
end
on("calendar", open_today)
hooks.calendar = open_today
on("cal_today", function()
    local y, m, d = today()
    local was = cal.year * 12 + cal.month
    show_month(y, m, key(y, m, d))
    if y * 12 + m > was then emit("cal_went_next") elseif y * 12 + m < was then emit("cal_went_prev") end
end)
on("cal_prev", function()
    local y, m = cal.year, cal.month - 1
    if m < 1 then y, m = y - 1, 12 end
    show_month(y, m)
    emit("cal_went_prev")
end)
on("cal_next", function()
    local y, m = cal.year, cal.month + 1
    if m > 12 then y, m = y + 1, 1 end
    show_month(y, m)
    emit("cal_went_next")
end)
--  A day of the grid: chosen; if it belongs to the month before or after, that
--  month comes in.
on("cal_pick", function(k)
    k = math.floor(k or 0)
    local lead = weekday(cal.year, cal.month, 1)
    local n = k - lead + 1
    local y, m = cal.year, cal.month
    local went = nil
    if n < 1 then
        m = m - 1
        if m < 1 then y, m = y - 1, 12 end
        n = days_in(y, m) + n
        went = "cal_went_prev"
    elseif n > days_in(y, m) then
        n = n - days_in(y, m)
        m = m + 1
        if m > 12 then y, m = y + 1, 1 end
        went = "cal_went_next"
    end
    show_month(y, m, key(y, m, n))
    if went then emit(went) end
end)
--  «10:30 Dentist», «9:05 call», «18h gym», «18.30 dinner»; anything else is
--  for the whole day.
on("submit:cal_new", function(value)
    local line = (value or ""):match("^%s*(.-)%s*$")
    if line == "" then return end
    local time, title = "", line
    local h, mi, rest = line:match("^(%d%d?)[:%.h](%d%d)%s+(.+)$")
    if not h then
        h, rest = line:match("^(%d%d?)h%s+(.+)$")
        mi = "00"
    end
    if h and tonumber(h) < 24 and tonumber(mi) < 60 then
        time, title = string.format("%02d:%s", tonumber(h), mi), rest
    end
    table.insert(cal.events, { date = cal.sel, time = time, title = title })
    save_calendar()
    text.cal_new = ""
    paint_calendar()
    emit("cal_added")
end)
on("cal_delete", function(i)
    local e = cal.rows[(i or 0) + 1]
    if e == nil then return end
    for k, x in ipairs(cal.events) do
        if x == e then table.remove(cal.events, k); break end
    end
    save_calendar()
    paint_calendar()
end)
--  When its time comes: once, and marked so that restarting her does not say
--  it again.
every(20000, function()
    local y, m, d = today()
    local t = os.date("*t")
    local now = t.hour * 60 + t.min
    local k = key(y, m, d)
    local changed = false
    for _, e in ipairs(cal.events) do
        if e.date == k and e.time ~= "" and not e.told then
            local eh, em = e.time:match("^(%d+):(%d+)$")
            local at = tonumber(eh) * 60 + tonumber(em)
            if now >= at and now < at + 5 then
                run("notify-send", { "-a", "Calendar", e.title, e.time }, function() end)
                e.told = true
                changed = true
            end
        end
    end
    if changed then save_calendar() end
end)
do
    local y, m, d = today()
    show_month(y, m, key(y, m, d))
end
on("open_page", function(k) log("page", k) end)

-- ── the session ────────────────────────────────────────────────────
--  The only thing in the whole scene that cannot be undone, so it wears two
--  belts. The first one is the sheet's and lives in the scene: logging out,
--  rebooting and powering off open a separate question, and the answer needs
--  a new press. The second one is from here: with `MAREA_TESTING` set, what
--  would be done is noted down and not done, which is what lets you rehearse
--  the whole gesture without being left without a machine halfway through the
--  test.
-- ── the lock ──────────────────────────────────────────────────────
--  Marea locks the door herself —the scene, with `kind: lock`—; here it is
--  only asked whether the password is the right one. Who gets in is decided by
--  PAM, and it takes its time on purpose when it is not: meanwhile, the scene
--  stays alive and says it is checking.
--  The emergency exit is its own thing, not the testing mode's: `MAREA_TESTING`
--  makes powering off and rebooting only be noted down, and that has no reason
--  to go together with the lock opening with Esc. The first real locks carry
--  the exit; when the variable is removed, it stops being there.
fact.with_exit = (sys.ask("env", "MAREA_LOCK_WITH_EXIT") or "") ~= "" or (sys.ask("env", "MAREA_TESTING") or "") ~= ""
--  A rule cannot touch a text, so emptying the box is this side's business:
--  when the lock goes on and when it comes off, let nothing be left written.
on("fact:locked", function() text.password = "" end)
on("submit:password", function(value)
    if fact.locked ~= true or fact.checking == true then return end
    if value == nil or value == "" then return end
    fact.checking = true
    --  Whatever happens with the check, «checking» does not stay on: with it
    --  on you cannot try again, and a lock screen that does not let you try
    --  again is a broken lock screen.
    after(8000, function() fact.checking = false end)
    local ok, valid = pcall(sys.ask, "auth.check", value)
    text.password = ""
    fact.checking = false
    if ok and valid == true then
        fact.locked = false
    else
        if not ok then log("lock · the password could not be checked:", valid) end
        emit("wrong_password")
    end
end)

local commands = { "session.lock", "session.suspend", "session.logout", "session.reboot", "session.poweroff" }
local testing = sys.ask("env", "MAREA_TESTING")

on("session", function(k)
    local command = commands[k]
    if command == nil then
        log("session: there is no action number", k)
        return
    end
    if testing ~= nil and testing ~= "" then
        log("session · testing: it would call", command)
        return
    end
    --  Once and only once: «after confirming, send the request a single time».
    log("session ·", command)
    sys.call(command)
end)

-- ── the system tray ────────────────────────────────────────────────
--  Which icons there are is said by pleamar; here it is decided what happens
--  when they are touched. A menu is a tree, and a submenu is ENTERED: `path` is
--  the way down so far, and the first row goes back. That way everything fits
--  on the same page.
local owner = nil                 -- whose menu is being shown
local root, path = {}, {}

local function show_menu(list)
    local rows = #path > 0 and { { back = true, label = tr("‹  Back") } } or {}
    for _, it in ipairs(list) do rows[#rows + 1] = it end
    model.options = rows
end

if not sys.watch("tray", function(list) model.icons = list end) then
    log("this system has no tray")
end

on("systray_activate", function(i)
    local it = model.icons[i + 1]
    if it then sys.call("tray.activate", it.key) end
end)

on("systray_menu", function(i)
    local it = model.icons[i + 1]
    if it == nil then return end
    model.options = {}
    text["systray.title"] = it.title ~= "" and it.title or it.id
    local ok, menu = pcall(sys.ask, "tray.menu", it.key)
    if not ok or menu == nil or #menu == 0 then
        --  There is no menu to read: let it show it itself, if it knows how,
        --  and no empty page is left here acting as a waiting room.
        pcall(sys.call, "tray.context", it.key)
        fact.page = "none"
        return
    end
    owner, root, path = it, menu, {}
    show_menu(menu)
end)

on("systray_pick", function(i)
    local r = model.options[i + 1]
    if r == nil or owner == nil then return end
    if r.back then
        table.remove(path)
        show_menu(#path > 0 and path[#path].children or root)
    elseif r.children and #r.children > 0 then
        path[#path + 1] = r
        show_menu(r.children)
    else
        sys.call("tray.menu_click", owner.key, r.id)
        fact.page = "none"
    end
end)

-- ── the photograph ─────────────────────────────────────────────────
--  The whole design rests on a decision that comes from Marea: you do not
--  frame the live desktop, you frame a FROZEN one. She leaves the frame, a
--  photo of everything is taken, and that photo is what gets cropped. That way
--  a menu that closes on the first click, a video that keeps playing or a
--  notice that arrives mid-selection cannot spoil anything: what comes out was
--  already done.
--
--  The scene says `gone` when she is no longer there; here the screen is
--  frozen, she is told `frozen` so that she comes back —turned into a camera—,
--  the crop is worked out according to what was asked for, and `shoot` is the
--  shutter's click.
--  Who says how the desktop is —the monitors, which one has the focus, where
--  the active window is—: Hyprland when she lives on it; else pleamar-wm's own
--  session, which says it in the same words (`pleamar-wm hyprctl …`).
local on_hyprland = (sys.ask("env", "HYPRLAND_INSTANCE_SIGNATURE") or "") ~= ""
local function hyprctl(what, done)
    if on_hyprland then
        run("hyprctl", { what }, done)
    else
        run("pleamar-wm", { "hyprctl", what }, done)
    end
end

-- Halley does not advertise foreign-toplevel-management. The adapter uses its
-- public SDK for focus and ordinary Wayland output metadata for capture bounds.
local function monitor_info(done)
    if on_halley then
        run("marea-halley", { "outputs" }, function(out, code)
            local ok, list = pcall(json.decode, out or "")
            if code ~= 0 or not ok or type(list) ~= "table" then
                log("Halley · could not enumerate outputs")
                done({})
                return
            end
            done(list)
        end)
    else
        hyprctl("monitors", function(out)
            local list = {}
            for block in ((out or "") .. "\n\n"):gmatch("(Monitor .-)\n\n") do
                local w, h, x, y = block:match("(%d+)x(%d+)@[%d%.]+ at (%-?%d+)x(%-?%d+)")
                if w then
                    list[#list + 1] = { name = block:match("Monitor (%S+)"),
                        x = tonumber(x), y = tonumber(y), w = tonumber(w), h = tonumber(h),
                        focused = block:find("focused: yes") ~= nil }
                end
            end
            done(list)
        end)
    end
end

--  In the pictures folder the session names —`~/Pictures` in English,
--  `~/Imágenes` in Spanish—, as the videos do.
local PHOTOS = home_dir .. "/Imágenes/Marea"
local FRAME = (sys.ask("env", "XDG_RUNTIME_DIR") or "/tmp") .. "/marea-plm-frame.png"
run("xdg-user-dir", { "PICTURES" }, function(d)
    if d and d ~= "" then PHOTOS = d .. "/Marea" end
    run("mkdir", { "-p", PHOTOS })
end)

--  The monitors: where the plane starts —`grim` photographs it whole and its
--  corner is the one of the monitor furthest up and to the left— and which one
--  has the focus.
local function parse_monitors(list)
    local minx, miny, focused = 0, 0, nil
    for k, monitor in ipairs(list) do
        if k == 1 and on_halley then minx, miny = monitor.x, monitor.y end
        minx, miny = math.min(minx, monitor.x), math.min(miny, monitor.y)
        if monitor.focused then focused = monitor end
    end
    if focused == nil and on_halley and #list == 1 then focused = list[1] end
    return minx, miny, focused
end

local function save(x, y, w, h)
    if w == nil or w < 2 or h < 2 then emit("shot_cancelled"); return end
    local dest = PHOTOS .. "/marea-" .. os.date("%Y%m%d-%H%M%S") .. ".png"
    run("magick", { FRAME, "-crop", string.format("%dx%d+%d+%d", w, h, x, y), "+repage", dest }, function(_, code)
        if code ~= 0 then log("photo · could not be cropped"); emit("shot_cancelled"); return end
        emit("shoot")
        --  To the clipboard too. `wl-copy` stays in the background serving what
        --  was copied, so there is no waiting for what it writes.
        run("wl-copy", { "--type", "image/png" }, nil, { stdin = dest, output = false })
        log("photo ·", dest)
    end)
end

on("gone", function()
    if fact.purpose ~= "shot" then return end
    local scope = fact.scope
    if on_halley and scope == "active_window" then
        -- SDK node geometry is not the presented rectangle. Guessing from it
        -- would capture the wrong region after camera motion or fullscreen.
        log("Halley · focused-window capture needs presented geometry, which API v1 does not expose")
        emit("shot_cancelled")
        return
    end
    --  A breather so the compositor can take away whatever was closing.
    after(140, function()
        --  The active window is asked BEFORE freezing: it is the one that was there.
        local function capture(window)
            monitor_info(function(mons)
                local minx, miny, focused = parse_monitors(mons)
                if on_halley and (#mons == 0 or (scope == "display" and not focused)) then
                    log("Halley · no focused output is available for capture")
                    emit("shot_cancelled")
                    return
                end
                run("grim", { FRAME }, function(_, code)
                    if code ~= 0 then log("photo · grim could not freeze the screen"); emit("shot_cancelled"); return end
                    emit("frozen")
                    if scope == "display" and focused then
                        save(focused.x - minx, focused.y - miny, focused.w, focused.h)
                    elseif scope == "active_window" then
                        local ax, ay = (window or ""):match("at: (%-?%d+),(%-?%d+)")
                        local aw, ah = (window or ""):match("size: (%d+),(%d+)")
                        if ax then save(tonumber(ax) - minx, tonumber(ay) - miny, tonumber(aw), tonumber(ah))
                        else emit("shot_cancelled") end
                    else
                        --  `slurp` gives «X,Y WxH» on the whole plane; Esc leaves with nothing.
                        run("slurp", {}, function(sel, c2)
                            local x, y, w, h = (sel or ""):match("(%-?%d+),(%-?%d+) (%d+)x(%d+)")
                            if c2 ~= 0 or x == nil then emit("shot_cancelled"); return end
                            save(tonumber(x) - minx, tonumber(y) - miny, tonumber(w), tonumber(h))
                        end)
                    end
                end)
            end)
        end
        if on_halley then capture(nil) else hyprctl("activewindow", capture) end
    end)
end)

-- ── the recording ──────────────────────────────────────────────────
--  Marea already learned the hard part, and here it is copied as it is:
--  · The countdown happens BEFORE starting the recorder: three seconds to move
--    the mouse out of the way, and none of them in the video because there is
--    no video yet.
--  · Constant frame rate (`-r 60 -D`). Without it `wf-recorder` only emits a
--    frame when something changes and the video's clock stops being the real one.
--  · Stopping is SIGINT and never killing: an MP4 writes its index at the end,
--    and without an index nobody can open it. That is why the recording is not
--    considered finished when «Stop» is pressed, but when the recorder HAS EXITED.
local VIDEOS = home_dir .. "/Vídeos/Marea"
run("xdg-user-dir", { "VIDEOS" }, function(d)
    if d and d ~= "" then VIDEOS = d .. "/Marea" end
    run("mkdir", { "-p", VIDEOS })
end)
local nvenc = false
run("test", { "-e", "/proc/driver/nvidia/version" }, function(_, code) nvenc = code == 0 end)
--  With the system's sound —what is playing, not the microphone, which is not
--  opened—, which is how Marea recorded. It is asked when going to record,
--  because the current output may have changed since it started.

local recorder, video = nil, nil
--  The last one saved, for the card's buttons.
local last_video = nil
on("reel_watch", function() if last_video then run("xdg-open", { last_video }, function() end) end end)
on("reel_folder", function() run("xdg-open", { VIDEOS }, function() end) end)
on("reel_copy", function() if last_video then run("wl-copy", { last_video }, function() end) end end)

on("fact:face", function(face)
    if face ~= "rec" or recorder ~= nil then return end
    run("pactl", { "get-default-sink" }, function(sound_sink, sink_code)
    if sink_code ~= 0 or sound_sink == "" then sound_sink = nil end
    monitor_info(function(mons)
        if fact.face ~= "rec" then return end          -- she changed her mind during the countdown
        local monitor = nil
        for _, output in ipairs(mons) do
            if output.focused then monitor = output.name end
        end
        if monitor == nil and on_halley and #mons == 1 then monitor = mons[1].name end
        if on_halley and monitor == nil then
            log("Halley · no focused output is available for recording")
            emit("stop")
            return
        end
        video = VIDEOS .. "/marea-" .. os.date("%Y%m%d-%H%M%S") .. ".mp4"
        local args = { "-f", video }
        local codec = nvenc and { "-c", "h264_nvenc", "-p", "preset=p5", "-p", "rc=vbr", "-p", "cq=23" }
                            or { "-c", "libx264", "-p", "preset=veryfast", "-p", "crf=23" }
        for _, a in ipairs(codec) do args[#args + 1] = a end
        for _, a in ipairs({ "-r", "60", "-D" }) do args[#args + 1] = a end
        if monitor then args[#args + 1] = "-o"; args[#args + 1] = monitor end
        --  `--audio=DEVICE` with the value attached: that is how `wf-recorder`
        --  documents it, and separated it silently takes it as «the default
        --  device», which is the microphone.
        if sound_sink then args[#args + 1] = "--audio=" .. sound_sink .. ".monitor" end
        local this_one
        --  Whether it got to record: kept here, not read from `taping`, which the
        --  scene switches off the moment «stop» is pressed —long before the
        --  recorder has finished—, so a finished recording never had its party.
        local reached = false
        local this_video = video
        this_one = spawn("wf-recorder", args, function() end, function(_, code)
            --  Now it really has finished, with its index written.
            if recorder == this_one then recorder = nil end
            local recorded = reached
            fact.taping = false
            log("recording · saved", video, "· exit", code)
            --  If it left on its own —it could not start—, let her not stay in
            --  the corner recording nothing.
            if fact.face == "rec" then emit("stop") end
            --  A video really made —it was recording, and the recorder closed it
            --  cleanly— deserves a party. Once she is back from her corner: she
            --  comes down 300 ms after `stop`, and the fireworks are hers.
            --  And the card that comes out of her: watch it, its folder, or its path.
            if recorded and (code == nil or code == 0) then
                last_video = this_video
                text["reel.name"] = this_video:match("[^/]+$") or this_video
                after(700, function()
                    emit("celebrate")
                    emit("recorded")
                end)
            end
        end)
        recorder = this_one
        --  «Really running»: if it is still alive after 400 ms, the red is its own.
        after(400, function()
            if recorder == this_one then
                fact.taping = true
                reached = true
            end
        end)
    end)
    end)
end)

on("stop", function()
    if recorder ~= nil then kill(recorder, "int") end
end)

-- ── the desktop wallpaper ──────────────────────────────────────────
--  Whoever paints it is not her: it is `swaybg`, and that is the reason the
--  wallpaper stays on when she is not there. If Marea drew it, closing her
--  would leave the desktop black, which is making worse something that
--  worked. Here it is only checked at startup whether someone is painting it;
--  if not —a fresh login—, the saved one is put on.
--
--  Two things learned in Marea, both about `swaybg`: ONE single call with all
--  the screens —starting it twice leaves the second one without the first
--  image—, and the PATH is saved, not a copy of anybody's file.
--
--  The settings live in `settings.json`: `home`, `follow`, `wallpaper`,
--  `skin` and `language`. They used to live in `ajustes.json`, with Spanish keys (`casa`,
--  `sigue`, `fondo`, `piel`) and Spanish skins (`lente`, `liquido`, `normal`):
--  if there is no new file yet, the old one is read and translated, and from
--  then on only the new one is written. The old one is left where it was.
local settings = {}
do
    local ok, loaded = pcall(sys.ask, "files.read", "settings.json", "json")
    if ok and type(loaded) == "table" then
        settings = loaded
    else
        local old_ok, old = pcall(sys.ask, "files.read", "ajustes.json", "json")
        if old_ok and type(old) == "table" then
            local OLD_SKINS = { lente = "lens", liquido = "liquid", normal = "classic" }
            settings = {
                home = old.casa,
                follow = old.sigue,
                wallpaper = old.fondo,
                skin = OLD_SKINS[old.piel] or old.piel,
            }
            log("settings · read from the old ajustes.json; from now on they go to settings.json")
        end
    end
end
local WALLPAPER = settings.wallpaper or "/usr/share/wallpapers/cachyos-wallpapers/GreenFeathers.png"

--  Only the `swaybg` of HER desktop (her `WAYLAND_DISPLAY`): one painting
--  another session's —Hyprland's, beside a pleamar-wm on another TTY— is not hers.
local OUR_DISPLAY = "WAYLAND_DISPLAY=" .. tostring(sys.ask("env", "WAYLAND_DISPLAY") or "")
run("pgrep", { "-x", "swaybg", "--env", OUR_DISPLAY }, function(_, code)
    if code == 0 then return end                        -- someone is already painting it
    monitor_info(function(mons)
        local args = {}
        for _, monitor in ipairs(mons) do
            for _, a in ipairs({ "-o", monitor.name, "-i", WALLPAPER, "-m", "fill" }) do args[#args + 1] = a end
        end
        if #args == 0 then return end
        --  With `run` and without collecting its output: that way it does not
        --  die with her, which is exactly what you want from a wallpaper.
        run("swaybg", args, nil, { output = false })
        log("wallpaper ·", WALLPAPER)
    end)
end)

-- ── her home: which monitor she lives on ───────────────────────────
--  One copy per monitor, and here it is decided which one is seen. Two ways: a
--  fixed home, or «follow me» —she goes wherever the window with the focus is,
--  which the `window` service tells—. The home is saved by NAME (`DP-3`):
--  monitors change number when another one is plugged in, and a name does not.
local monitors = {}                  -- in the compositor's order: name
local focused = ""
local pointed = nil                  -- pleamar-wm: the monitor the mouse is on, by name

--  In pleamar-wm's session, where she is is also where a window put away
--  flies to: its window manager is told by number, in its own order of the
--  monitors (the one `pleamar-wm hyprctl monitors` gives).
local told_island = nil
local function tell_island()
    if not in_wm then return end
    local name = nil
    for k = 0, 1 do
        if fact["hosts." .. k] == true then name = monitors[k + 1] end
    end
    if name == nil or name == told_island then return end
    told_island = name
    run("pleamar-wm", { "hyprctl", "monitors" }, function(out)
        for n, id in tostring(out or ""):gmatch("Monitor (%S+) %(ID (%d+)%)") do
            if n == name then
                run("pleamar-wm", { "--say", "wm", "fact island.screen " .. id }, nil, { output = false })
            end
        end
    end)
end

local function decide()
    --  In pleamar-wm's session she follows the mouse; elsewhere, the focus.
    local wanted = pointed or focused
    for k = 0, 1 do
        local name = monitors[k + 1]
        local hers
        if fact.following == true then
            hers = name ~= nil and name == wanted
            --  With no focus on any —the empty desktop—, home.
            if wanted == "" then hers = k == fact.home end
        else
            hers = k == fact.home
        end
        fact["hosts." .. k] = hers
    end
    tell_island()
end

--  Her wardrobe's pieces, as the scene names them (`wears.hat`…).
local PIECES = { "hat", "glasses", "mug", "phones" }

local function save_settings()
    settings.home = monitors[(fact.home or 0) + 1] or settings.home
    settings.follow = fact.following == true
    settings.room = fact.taking_room == true
    settings.skin = fact.skin
    settings.language = fact.language
    settings.wallpaper = WALLPAPER
    settings.dnd = fact.dnd == true
    settings.quiet_rec = fact.quiet_rec ~= false
    settings.snooze = math.floor(fact.snooze_k or 1)
    --  What she has on: it stays on through a restart.
    settings.wears = {}
    for _, piece in ipairs(PIECES) do settings.wears[piece] = fact["wears." .. piece] == true end
    local list = {}
    for app in pairs(muted_apps) do list[#list + 1] = app end
    table.sort(list)
    settings.muted = list
    pcall(sys.call, "files.write", "settings.json", settings)
end

-- ── quiet notices: do not disturb, later, muted apps ──────────────
local function say(words)
    fact["drift.result"] = 1
    text["drift.toast"] = words
    emit("say_toast")
end
local function paint_muted()
    local list = {}
    for app in pairs(muted_apps) do list[#list + 1] = app end
    table.sort(list)
    local shown_list = {}
    for k = 1, math.min(#list, 6) do shown_list[k] = { app = list[k] } end
    model.muted = shown_list
    return list
end
fact.dnd = settings.dnd == true
fact.quiet_rec = settings.quiet_rec ~= false
fact.snooze_k = math.clamp(tonumber(settings.snooze) or 1, 0, 2)
for _, app in ipairs(type(settings.muted) == "table" and settings.muted or {}) do muted_apps[tostring(app)] = true end
paint_muted()
on("fact:dnd", function() save_settings() end)
on("fact:quiet_rec", function() save_settings() end)
on("fact:snooze_k", function() save_settings() end)

--  «Later»: the notice leaves, without going to the history, and comes back
--  —as if it had just arrived— once its time is up, if it is still there.
local SNOOZE_MINUTES = { 15, 60, 180 }
local SNOOZE_WORDS = { "Back in 15 minutes", "Back in an hour", "Back in three hours" }
hooks.snooze = function(f)
    f.done = true
    local k = math.floor(fact.snooze_k or 1) + 1
    say(tr(SNOOZE_WORDS[k]))
    if f.id == nil then return end
    local id = f.id
    snoozed[id] = true
    cap_counts()
    after(SNOOZE_MINUTES[k] * 60000, function()
        snoozed[id] = nil
        for _, n in ipairs(live or {}) do
            if n.id == id then
                remove_done()
                arrive(row_of(n))
                return
            end
        end
    end)
end
--  «Mute»: that app's notices, the ones there and the ones to come, go
--  straight to the history.
hooks.mute = function(app)
    if app == nil or app == "" then return end
    muted_apps[app] = true
    for _, r in ipairs(rows) do
        if r.app == app and not r.done then
            r.done = true
            remember(r)
            if r.id then finished[r.id] = true; sys.call("notifications.dismiss", r.id) end
        end
    end
    paint_muted()
    save_settings()
    say(string.format(tr("%s muted"), app))
end
on("unmute", function(i)
    local list = paint_muted()
    local app = list[math.floor(i or 0) + 1]
    if app == nil then return end
    muted_apps[app] = nil
    paint_muted()
    save_settings()
end)
hooks.dnd = function()
    fact.dnd = not (fact.dnd == true)
    say(tr(fact.dnd and "Do not disturb: on" or "Do not disturb: off"))
end

-- ── the wallpapers, and the tide that brings a new one ────────────
--  The ones there are: in the usual folders, not the screenshots. Small
--  copies of them are made with `magick` into the cache as they are seen, a
--  few at a time; and picking one makes two more, the old and the new one at
--  the monitor's size and cut the way `swaybg -m fill` cuts them, so that
--  when the tide has passed and `swaybg` takes over nothing moves.
local WALL_CACHE = home_dir .. "/.cache/marea/walls"
run("mkdir", { "-p", WALL_CACHE }, function() end)
local wall_list, wall_offset, wall_thumbs, wall_making = {}, 0, {}, {}
local wall_busy = false

local function cache_name(path, tag)
    return WALL_CACHE .. "/" .. tag .. "-" .. tostring(path):gsub("[^%w]", "_"):sub(-120) .. ".jpg"
end

local function show_walls()
    local cards = {}
    for k = wall_offset + 1, math.min(wall_offset + 9, #wall_list) do
        local path = wall_list[k]
        local name = tostring(path):match("([^/]+)$") or path
        cards[#cards + 1] = { name = name:gsub("%.%w+$", ""):gsub("[_%-]", " "), thumb = wall_thumbs[path] or "",
                              ready = wall_thumbs[path] ~= nil, current = path == WALLPAPER }
    end
    model.walls = cards
end

--  The small copies of what is on the page, three at a time.
local function make_thumbs()
    local going = 0
    for _ in pairs(wall_making) do going = going + 1 end
    for k = wall_offset + 1, math.min(wall_offset + 9, #wall_list) do
        if going >= 3 then return end
        local path = wall_list[k]
        if wall_thumbs[path] == nil and not wall_making[path] then
            local thumb = cache_name(path, "thumb")
            wall_making[path] = true
            going = going + 1
            run("test", { "-s", thumb }, function(_, exists)
                local function done()
                    wall_making[path] = nil
                    wall_thumbs[path] = thumb
                    show_walls()
                    make_thumbs()
                end
                if exists == 0 then return done() end
                run("magick", { path .. "[0]", "-thumbnail", "320x180^", "-gravity", "center", "-extent", "320x180", "-quality", "85", thumb }, function(_, code)
                    if code ~= 0 then wall_making[path] = nil; log("wallpapers · no small copy of", path); return end
                    done()
                end)
            end)
        end
    end
end

--  The one on now, for her wardrobe: its small copy, made if it is not there.
local function show_wall_now()
    local path = WALLPAPER
    local name = (tostring(path):match("([^/]+)$") or path):gsub("%.%w+$", ""):gsub("[_%-]", " ")
    local thumb = cache_name(path, "thumb")
    model.wallnow = { { name = name, thumb = wall_thumbs[path] or "", ready = wall_thumbs[path] ~= nil } }
    if wall_thumbs[path] ~= nil then return end
    run("test", { "-s", thumb }, function(_, exists)
        local function done()
            wall_thumbs[path] = thumb
            if path == WALLPAPER then model.wallnow = { { name = name, thumb = thumb, ready = true } } end
        end
        if exists == 0 then return done() end
        run("magick", { path .. "[0]", "-thumbnail", "320x180^", "-gravity", "center", "-extent", "320x180", "-quality", "85", thumb }, function(_, code)
            if code == 0 then done() end
        end)
    end)
end
on("customize", show_wall_now)

local WALL_DIRS = { home_dir .. "/Imágenes", home_dir .. "/Pictures", home_dir .. "/Wallpapers", home_dir .. "/Fondos",
                    "/usr/share/wallpapers", "/usr/share/backgrounds" }
on("walls_open", function()
    local args = { "--type", "f", "--max-depth", "3", "--color", "never",
                   "-e", "png", "-e", "jpg", "-e", "jpeg", "-e", "webp",
                   "--exclude", "Capturas", "--exclude", "Screenshots", "--exclude", "Marea", "--exclude", "pokes", ".", }
    for _, d in ipairs(WALL_DIRS) do args[#args + 1] = d end
    run("fd", args, function(out)
        local list, seen_walls = {}, {}
        for line in tostring(out or ""):gmatch("[^\n]+") do
            if not seen_walls[line] then seen_walls[line] = true; list[#list + 1] = line end
        end
        table.sort(list, function(a, b) return (a:match("([^/]+)$") or a):lower() < (b:match("([^/]+)$") or b):lower() end)
        --  The one on now, first.
        for k, path in ipairs(list) do
            if path == WALLPAPER then table.remove(list, k); table.insert(list, 1, path); break end
        end
        wall_list = list
        wall_offset = 0
        show_walls()
        make_thumbs()
    end)
end)
on("walls_scroll", function(w)
    local step = (w or 0) > 0 and -3 or 3
    wall_offset = math.max(0, math.min(wall_offset + step, math.max(#wall_list - 3, 0)))
    wall_offset = wall_offset - wall_offset % 3
    show_walls()
    make_thumbs()
end)

--  The picture the tide carries: cut as `swaybg -m fill` cuts it for the
--  monitor, and squeezed into the 16:9 the scene holds it in (it is drawn back
--  at the monitor's shape). Always the same file: a new version of it, `?n`,
--  takes the place of the last one, and the atlas does not fill up.
local TIDE_PIC = WALL_CACHE .. "/tide.jpg"
local tide_version = 0
local function cover(path, done)
    local w = math.floor(fact["screen.0.width"] or 0)
    local h = math.floor(fact["screen.0.height"] or 0)
    if w < 100 or h < 100 then w, h = 1920, 1080 end
    run("magick", { path .. "[0]", "-resize", w .. "x" .. h .. "^", "-gravity", "center", "-extent", w .. "x" .. h,
        "-resize", "1280x720!", "-quality", "90", TIDE_PIC }, function(_, code)
        tide_version = tide_version + 1
        done(code == 0 and (TIDE_PIC .. "?" .. tide_version) or nil)
    end)
end

local tide_new = nil
on("wall_pick", function(i)
    local path = wall_list[wall_offset + i + 1]
    if path == nil or path == WALLPAPER or wall_busy then return end
    wall_busy = true
    tide_new = path
    cover(path, function(pic)
        if pic == nil then
            log("wallpapers · the picture for the tide could not be made; straight to it")
            emit("tide_done")
            return
        end
        model.tidepics = { { pic = pic } }
        fact["tide.home"] = fact["hosts.1"] == true and 1 or 0
        emit("tide_start")
    end)
end)

--  The water has settled: `swaybg` with the new one, and the old one goes a
--  moment later, when the new one is surely painted underneath.
on("tide_done", function()
    local path = tide_new
    if path == nil then return end
    tide_new = nil
    --  Without `-o`, one swaybg paints every monitor there is.
    run("pgrep", { "-x", "swaybg", "--env", OUR_DISPLAY }, function(old_pids)
        run("swaybg", { "-i", path, "-m", "fill" }, nil, { output = false })
        WALLPAPER = path
        save_settings()
        show_wall_now()
        after(700, function()
            local pids = {}
            for pid in tostring(old_pids or ""):gmatch("%d+") do pids[#pids + 1] = pid end
            if #pids > 0 then run("kill", pids, function() end) end
            emit("tide_away")
            wall_busy = false
            if fact.page == "walls" then show_walls() end
        end)
    end)
end)

--  Which monitor each copy is, pleamar says, not `hyprctl`: the order of the
--  copies is its own (`screen.0.name`, `screen.1.name`), and it does not match
--  the compositor's. They arrive as each surface is put up, so the home is
--  picked up as soon as both are known.
local function place()
    fact.home = 0
    for k, name in ipairs(monitors) do
        if name == settings.home then fact.home = k - 1 end
    end
    fact.following = settings.follow == true
    fact.taking_room = settings.room == true
    decide()
end
for k = 0, 1 do
    on("text:screen." .. k .. ".name", function(name)
        if name == nil or name == "" then return end
        monitors[k + 1] = name
        place()
    end)
end
--  When only the logic is reloaded, the names no longer arrive again: the ones
--  there are are read.
for k = 0, 1 do
    local name = text["screen." .. k .. ".name"]
    if name ~= nil and name ~= "" then monitors[k + 1] = name end
end

-- ── the stones: windows put away ───────────────────────────────────
--  pleamar-wm's session lists every window through the `window` service, and
--  says which are put away (minimized): each one gets a slot of its own —the
--  first free— and keeps it until it comes back, so a stone never changes
--  face while the others close ranks. Its face is its application's icon,
--  found by the window's app id (the `.desktop` file's name, or its X11
--  class); without one, the initial.
local stone_slots = {}                -- slot (1..6) → window id
local stone_seq = 0                   -- how many have been put away: the newest sits next to her
local stone_faces = {}                -- slot → the record shown
local function face_of(class)
    local c = (class or ""):lower()
    if c == "" then return "" end
    for _, a in ipairs(apps) do
        local id = (a.id or ""):lower()
        local wm = (a.wmclass or ""):lower()
        local bin = ((a.exec or ""):match("^(%S+)") or ""):match("([^/]+)$") or ""
        if id == c or wm == c or id:sub(-#c - 1) == "." .. c or bin:lower() == c then
            return a.icon or ""
        end
    end
    return ""
end
local function set_stones(list)
    local away = {}
    for _, w in ipairs(list or {}) do
        if w.minimized then away[w.id] = w end
    end
    for k = 1, 6 do
        local id = stone_slots[k]
        if id ~= nil and away[id] == nil then
            stone_slots[k] = nil
            if stone_faces[k] then stone_faces[k].present = false end
        end
    end
    for id, w in pairs(away) do
        local has = false
        for k = 1, 6 do if stone_slots[k] == id then has = true end end
        if not has then
            for k = 1, 6 do
                if stone_slots[k] == nil then
                    stone_slots[k] = id
                    local icon = face_of(w.class)
                    local name = (w.title ~= "" and w.title) or w.class or ""
                    stone_seq = stone_seq + 1
                    stone_faces[k] = { title = name, icon = icon, letter = icon == "" and name:sub(1, 1):upper() or "", present = true, seq = stone_seq }
                    break
                end
            end
        end
    end
    local out = {}
    for k = 1, 6 do
        out[k] = stone_faces[k] or { title = "", icon = "", letter = "", present = false, seq = 0 }
    end
    model.stones = out
end
hooks.restore_all = function()
    for k = 1, 6 do
        local id = stone_slots[k]
        if id ~= nil then window_call("restore", id) end
    end
end
on("restore_stone", function(k)
    local id = stone_slots[(k or 0) + 1]
    if id ~= nil then window_call("restore", id) end
end)

local function from_window(v)
    focused = (v and v.monitor) or ""
    open_windows = (v and v.list) or {}
    if fact.following == true then decide() end
    --  In pleamar-wm what is put away floats on the shore of its monitor
    --  (tiled, nothing is): her island keeps no stones there.
    set_stones((not in_wm) and v and v.list or {})
end
if on_halley then
    local function watch()
        local ok = pcall(spawn, "marea-halley", { "watch" }, function(line)
            local valid, value = pcall(json.decode, line)
            if valid and type(value) == "table" then from_window(value) end
        end, function()
            from_window(nil)
            after(1000, watch)
        end)
        if not ok then log("Halley · marea-halley is unavailable") end
    end
    watch()
elseif not sys.watch("window", from_window) then
    log("this compositor does not say where the focus is: «follow me» will not work")
end

--  Its number, in pleamar-wm's order of the monitors, to its name.
local function point_to(id)
    --  Only pleamar-wm's session says it; anywhere else it is someone
    --  else's word about monitors that are not hers.
    if not in_wm then return end
    run("pleamar-wm", { "hyprctl", "monitors" }, function(out)
        for n, k in tostring(out or ""):gmatch("Monitor (%S+) %(ID (%d+)%)") do
            if tonumber(k) == id and pointed ~= n then
                pointed = n
                if fact.following == true then decide() end
            end
        end
    end)
end
on("pointer_monitor", point_to)
--  Born into the session, she asks where the mouse is: it is only told
--  when it changes monitor, and until it did she went home —often the
--  other monitor, with the cursor waiting on this one—.
if in_wm then
    after(300, function()
        run("pleamar-wm", { "--say", "wm", "get mon" }, function(out)
            local id = tonumber((tostring(out or ""):match("%-?%d+")))
            if id ~= nil and id >= 0 then point_to(id) end
        end)
    end)
end

on("fact:home", function() decide(); save_settings() end)
on("fact:following", function() decide(); save_settings() end)
on("fact:taking_room", function() save_settings() end)

--  Her look, as it was left last time. If the file holds something that is not
--  one of the three —another version, a hand edit—, the factory one stays.
if settings.skin == "lens" or settings.skin == "liquid" or settings.skin == "classic" then
    fact.skin = settings.skin
end
on("fact:skin", function() save_settings() end)

-- ── her sky: the weather where you are ─────────────────────────────
--  The city is chosen in the calendar (its strip at the top) and kept in her
--  settings; without one, she guesses it from the time zone (Europe/Madrid:
--  Madrid). Open-Meteo is asked every quarter of an hour —no account, no
--  key— and the scene draws it on the time card.
local WX_WORDS = { [0] = "Clear sky", [1] = "Mostly clear", [2] = "Partly cloudy", [3] = "Overcast",
    [45] = "Fog", [48] = "Fog", [51] = "Drizzle", [53] = "Drizzle", [55] = "Drizzle", [56] = "Drizzle", [57] = "Drizzle",
    [61] = "Rain", [63] = "Rain", [65] = "Heavy rain", [66] = "Rain", [67] = "Rain",
    [71] = "Snow", [73] = "Snow", [75] = "Heavy snow", [77] = "Snow",
    [80] = "Showers", [81] = "Showers", [82] = "Heavy showers", [85] = "Snow showers", [86] = "Snow showers",
    [95] = "Thunderstorm", [96] = "Thunderstorm", [99] = "Thunderstorm" }
local WX_RAIN = { [51] = 0.3, [53] = 0.4, [55] = 0.5, [56] = 0.4, [57] = 0.5, [61] = 0.5, [63] = 0.75, [65] = 1,
    [66] = 0.6, [67] = 0.9, [80] = 0.6, [81] = 0.8, [82] = 1, [95] = 0.9, [96] = 1, [99] = 1 }
local WX_SNOW = { [71] = 0.4, [73] = 0.7, [75] = 1, [77] = 0.3, [85] = 0.6, [86] = 1 }
local sky_place = nil
local sky_code = nil
local function sky_words()
    text["wx.summary"] = sky_code ~= nil and tr(WX_WORDS[sky_code] or "Clear sky") or ""
end
local function url_encode(s)
    return (tostring(s):gsub("[^%w%-_%.~]", function(c) return string.format("%%%02X", string.byte(c)) end))
end
local function sky_fetch()
    if sky_place == nil then return end
    local url = string.format("https://api.open-meteo.com/v1/forecast?latitude=%.4f&longitude=%.4f&current=temperature_2m,weather_code,cloud_cover&daily=temperature_2m_max,temperature_2m_min&forecast_days=1&timezone=auto", sky_place.lat, sky_place.lon)
    run("curl", { "-s", "--max-time", "10", url }, function(out)
        local ok, v = pcall(json.decode, tostring(out or ""))
        if not ok or type(v) ~= "table" or type(v.current) ~= "table" then return end
        local c = v.current
        sky_code = tonumber(c.weather_code) or 0
        local rain, snow = WX_RAIN[sky_code] or 0, WX_SNOW[sky_code] or 0
        local cloud = (tonumber(c.cloud_cover) or 0) / 100
        if rain > 0 or snow > 0 then cloud = math.max(cloud, 0.8) end
        fact["wx.temp"] = tonumber(c.temperature_2m) or 0
        fact["wx.cloud"] = cloud
        fact["wx.rain"] = rain
        fact["wx.snow"] = snow
        fact["wx.fog"] = (sky_code == 45 or sky_code == 48) and 1 or 0
        fact["wx.storm"] = sky_code >= 95 and 1 or 0
        if type(v.daily) == "table" then
            fact["wx.hi"] = tonumber((v.daily.temperature_2m_max or {})[1]) or fact["wx.temp"]
            fact["wx.lo"] = tonumber((v.daily.temperature_2m_min or {})[1]) or fact["wx.temp"]
        end
        fact["wx.ok"] = true
        sky_words()
    end)
end
local function sky_find(name)
    local q = tostring(name or ""):match("^%s*(.-)%s*$")
    if q == "" then return end
    local lang = fact.locale == "es" and "es" or "en"
    run("curl", { "-s", "--max-time", "10", "https://geocoding-api.open-meteo.com/v1/search?count=1&format=json&language=" .. lang .. "&name=" .. url_encode(q) }, function(out)
        local ok, v = pcall(json.decode, tostring(out or ""))
        local r = ok and type(v) == "table" and type(v.results) == "table" and v.results[1] or nil
        if r == nil or r.latitude == nil then
            log("her sky · no city called", q)
            return
        end
        sky_place = { name = tostring(r.name), lat = tonumber(r.latitude), lon = tonumber(r.longitude) }
        settings.city = sky_place
        text["wx.place"] = sky_place.name
        save_settings()
        log("her sky ·", sky_place.name)
        sky_fetch()
    end)
end
if type(settings.city) == "table" and tonumber(settings.city.lat) ~= nil then
    sky_place = { name = tostring(settings.city.name or ""), lat = tonumber(settings.city.lat), lon = tonumber(settings.city.lon) }
    text["wx.place"] = sky_place.name
    sky_fetch()
else
    run("timedatectl", { "show", "-p", "Timezone", "--value" }, function(out)
        local city = tostring(out or ""):match("([^/%s]+)%s*$")
        if city then sky_find((city:gsub("_", " "))) end
    end)
end
every(15 * 60 * 1000, sky_fetch)
on("submit:city_q", function(value)
    sky_find(value)
    text.city_q = ""
end)
on("fact:locale", sky_words)

--  And what she had on, as it was left: each piece that is not there, off.
if type(settings.wears) == "table" then
    for _, piece in ipairs(PIECES) do
        if settings.wears[piece] == true then fact["wears." .. piece] = true end
    end
end
for _, piece in ipairs(PIECES) do
    on("fact:wears." .. piece, function() save_settings() end)
end

-- ── the language ───────────────────────────────────────────────────
--  What the scene writes changes language by itself; what this logic wrote
--  stays in the language it was written in until it is written again. So when
--  `locale` changes, the texts that are Marea's own words are written again:
--  the rows of the swell, her menu and the finder's list (only if it is open:
--  otherwise it is filled again when it opens).
on("fact:locale", function()
    render()
    set_menu()
    if fact.searching == true then search() end
end)

--  Which one. «System» is what the session says —`LC_ALL`, `LC_MESSAGES`,
--  `LANG`, the first one that says something, as pleamar reads it—, asked here
--  and not remembered from the `locale` it started with: if only the logic is
--  reloaded, `locale` may already be one chosen by hand. Anything that is not
--  Spanish is English, which is the language the scene is written in.
local function system_locale()
    for _, v in ipairs({ "LC_ALL", "LC_MESSAGES", "LANG" }) do
        local ok, value = pcall(sys.ask, "env", v)
        if ok and type(value) == "string" and value ~= "" and value ~= "C" and value ~= "POSIX" then
            return value:sub(1, 2):lower() == "es" and "es" or "en"
        end
    end
    return "en"
end
local LOCALES = { english = "en", spanish = "es" }
local function speak()
    fact.locale = LOCALES[fact.language] or system_locale()
end
if settings.language == "system" or settings.language == "english" or settings.language == "spanish" then
    fact.language = settings.language
end
speak()
on("fact:language", function() speak(); save_settings() end)
