-- Markdown highlighting plus ankh specifics.
vim.cmd([[
  runtime! syntax/markdown.vim
  syntax match ankhFieldHeading /^## .*$/
  syntax match ankhFrontmatterKey /^\(note\|notetype\|deck\|tags\):/ contained
  syntax region ankhFrontmatter start=/^---$/ end=/^---$/ contains=ankhFrontmatterKey keepend
  syntax match ankhCloze /{{c\d\+::[^}]*}}/
  syntax match ankhSound /\[sound:[^\]]*\]/
  syntax match ankhRaw /<!-- html -->/
  highlight default link ankhFieldHeading Title
  highlight default link ankhFrontmatter Comment
  highlight default link ankhFrontmatterKey Keyword
  highlight default link ankhCloze Special
  highlight default link ankhSound String
  highlight default link ankhRaw Todo
]])
