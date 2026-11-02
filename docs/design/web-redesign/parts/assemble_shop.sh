#!/bin/bash
# usage: assemble_shop.sh Name "Title" height
S=/private/tmp/claude-503/-Users-cotit-Documents-genesy--claude-worktrees-web-redesign-580857/aba4db97-726a-400e-acd7-ac09c2fc3058/scratchpad
P=$S/design/project
{ sed -e "s|<title>Shop catalogue</title>|<title>Shop $2</title>|" $S/parts/shop_head.html
  cat $S/parts/$1.body.html
  sed -e "s|\"height\":1700|\"height\":$3|" $S/parts/shop_tail.html
} > $P/$1.dc.html
grep -c "BODY" $P/$1.dc.html
