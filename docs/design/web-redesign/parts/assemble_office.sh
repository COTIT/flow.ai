#!/bin/bash
# usage: assemble_office.sh Name "Active nav" "Crumb" height
S=/private/tmp/claude-503/-Users-cotit-Documents-genesy--claude-worktrees-web-redesign-580857/aba4db97-726a-400e-acd7-ac09c2fc3058/scratchpad
P=$S/design/project
{ sed -e "s|<title>Office overview</title>|<title>Office $3</title>|" -e "s|<b>Overview</b>|<b>$3</b>|" $S/parts/office_head.html
  cat $S/parts/$1.body.html
  sed -e "s|const active = 'Overview'|const active = '$2'|" -e "s|\"height\":1160|\"height\":$4|" $S/parts/office_tail.html
} > $P/$1.dc.html
grep -c "page-head\|const active = '$2'" $P/$1.dc.html
