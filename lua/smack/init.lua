local M = {}

local uv = vim.uv or vim.loop

M.config = {
  socket_path = "/tmp/smack.sock",
  enabled = true,
  shake = true,
  undo_count = { light = 1, medium = 3, hard = 5 },
  shake_intensity = { light = 1, medium = 3, hard = 5 },
}

local pipe = nil
local buf = "" -- partial line buffer

-- ── Screen shake ────────────────────────────────────────────────────────────

local function screen_shake(intensity)
  local ok, view = pcall(vim.fn.winsaveview)
  if not ok or not view then return end

  -- Build an alternating offset pattern: +N, -N, +N, -N, ... , 0
  local offsets = {}
  for i = 1, intensity * 2 do
    local dir = (i % 2 == 0) and -1 or 1
    table.insert(offsets, dir * math.ceil(intensity / 2))
  end
  table.insert(offsets, 0)

  local step = 0
  local timer = uv.new_timer()
  timer:start(0, 25, vim.schedule_wrap(function()
    step = step + 1
    if step > #offsets then
      timer:stop()
      timer:close()
      pcall(vim.fn.winrestview, view)
      return
    end
    pcall(vim.fn.winrestview, {
      topline = math.max(1, view.topline + offsets[step]),
      lnum = view.lnum,
      col = view.col,
      curswant = view.curswant,
    })
  end))
end

-- ── Hit handler ─────────────────────────────────────────────────────────────

local function on_hit(event)
  if not M.config.enabled then return end

  local severity = event.severity or "light"
  local amplitude = event.amplitude or 0
  local undos = M.config.undo_count[severity] or 1
  local shake_int = M.config.shake_intensity[severity] or 1

  vim.schedule(function()
    -- Undo
    for _ = 1, undos do
      local ok = pcall(vim.cmd, "silent undo")
      if not ok then break end -- nothing left to undo
    end

    -- Shake
    if M.config.shake then
      screen_shake(shake_int)
    end

    vim.notify(
      string.format("SMACK! (%dx undo, %.2fg)", undos, amplitude),
      vim.log.levels.WARN
    )
  end)
end

-- ── Socket connection ───────────────────────────────────────────────────────

local function process_line(line)
  if line == "" then return end
  local ok, data = pcall(vim.json.decode, line)
  if ok and data and data.severity then
    on_hit(data)
  end
end

local function connect()
  if pipe then return end

  local p = uv.new_pipe(false)
  p:connect(M.config.socket_path, function(err)
    if err then
      p:close()
      vim.schedule(function()
        vim.notify(
          "smack.nvim: can't connect to smack — is it running? (sudo smack)",
          vim.log.levels.ERROR
        )
      end)
      return
    end

    pipe = p
    buf = ""

    vim.schedule(function()
      vim.notify("smack.nvim: connected", vim.log.levels.INFO)
    end)

    p:read_start(function(read_err, data)
      if read_err or not data then
        vim.schedule(function()
          vim.notify("smack.nvim: disconnected", vim.log.levels.WARN)
        end)
        pcall(function() p:read_stop() end)
        pcall(function() p:close() end)
        pipe = nil
        buf = ""
        return
      end

      -- Buffer partial lines (Unix socket can split/merge writes)
      buf = buf .. data
      while true do
        local nl = buf:find("\n")
        if not nl then break end
        local line = buf:sub(1, nl - 1)
        buf = buf:sub(nl + 1)
        process_line(line)
      end
    end)
  end)
end

local function disconnect()
  if pipe then
    pcall(function() pipe:read_stop() end)
    pcall(function() pipe:close() end)
    pipe = nil
    buf = ""
    vim.notify("smack.nvim: disconnected", vim.log.levels.INFO)
  end
end

-- ── Public API ──────────────────────────────────────────────────────────────

function M.start()
  connect()
end

function M.stop()
  disconnect()
end

function M.toggle()
  if pipe then
    disconnect()
  else
    connect()
  end
end

function M.setup(opts)
  M.config = vim.tbl_deep_extend("force", M.config, opts or {})

  vim.api.nvim_create_user_command("SmackStart", function() M.start() end, {})
  vim.api.nvim_create_user_command("SmackStop", function() M.stop() end, {})
  vim.api.nvim_create_user_command("SmackToggle", function() M.toggle() end, {})

  if M.config.enabled then
    vim.api.nvim_create_autocmd("VimEnter", {
      callback = function() M.start() end,
      once = true,
    })
  end

  vim.api.nvim_create_autocmd("VimLeavePre", {
    callback = function() M.stop() end,
  })
end

return M
