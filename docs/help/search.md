# Search syntax

ankh uses Anki's search language unchanged (it *is* Anki's parser).
Terms are ANDed; `or` and parentheses group; `-` negates; quote phrases.

## Text

| query | matches |
|-------|---------|
| `dog` | `dog` anywhere in any field |
| `"a phrase"` | the phrase |
| `d_g` `d*g` | one char / any chars |
| `front:dog` | field named Front is exactly `dog` |
| `front:*dog*` | field contains `dog` |
| `re:(a|b)` | regex |
| `nc:cafe` | ignore accents |
| `w:dog` | whole word |

## Where

`deck:Korean` `deck:Korean::Vocab` `deck:"with spaces"` `deck:cur`
`tag:leech` `tag:none` `tag:x*` `note:Basic` (notetype) `card:1` / `card:Forward` (template)

## State

`is:due` `is:new` `is:learn` `is:review` `is:suspended` `is:buried`
`flag:1`…`flag:7` `flag:0` `-flag:0`

## Time & scheduling

`added:7` (last 7 days) `edited:1` `rated:1` (answered today) `rated:7:1` (again, last week)
`prop:due=1` (tomorrow) `prop:due<=7` `prop:ivl>=30` `prop:reps>10` `prop:lapses>3`
`prop:ease<2` `prop:s>21` (FSRS stability) `prop:d>0.8` (difficulty) `prop:r<0.9` (retrievability)
`introduced:7` `resched:1`

## Ids & presets

`cid:123` `nid:123` `mid:123` `preset:Default` `has-cd:key` `dupe:mid,text`

## Examples

```
deck:Korean is:due -tag:leech
tag:leech prop:lapses>10
"deck:Korean::TTMIK Level 1" (is:new or prop:ivl<7)
added:30 note:"Korean Vocab"
```
