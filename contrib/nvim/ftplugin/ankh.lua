-- ankh note files: Markdown with frontmatter and `## Field` headings.
vim.bo.commentstring = "<!-- %s -->"
vim.bo.textwidth = 0
vim.wo.wrap = true
vim.wo.linebreak = true
vim.wo.conceallevel = 0
vim.opt_local.spell = false

-- Jump between fields.
vim.keymap.set("n", "]]", function() vim.fn.search("^## ", "W") end, { buffer = true, desc = "next field" })
vim.keymap.set("n", "[[", function() vim.fn.search("^## ", "bW") end, { buffer = true, desc = "previous field" })

-- Cloze helpers: wrap the visual selection in {{cN::...}}.
local function next_cloze()
  local n = 0
  for _, line in ipairs(vim.api.nvim_buf_get_lines(0, 0, -1, false)) do
    for c in line:gmatch("{{c(%d+)::") do n = math.max(n, tonumber(c)) end
  end
  return n + 1
end
local function wrap_cloze(same)
  local n = same and math.max(next_cloze() - 1, 1) or next_cloze()
  vim.cmd(string.format([[normal! gv"zc{{c%d::<C-r>z}}]], n))
end
vim.keymap.set("x", "<C-c>", function() wrap_cloze(false) end, { buffer = true, desc = "cloze (new number)" })
vim.keymap.set("x", "<C-S-c>", function() wrap_cloze(true) end, { buffer = true, desc = "cloze (same number)" })
