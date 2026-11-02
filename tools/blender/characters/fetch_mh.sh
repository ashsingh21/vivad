#!/bin/zsh
# Fetches the MakeHuman community asset packs (data only), each zip into its own folder and unpacked in x/.
set -u
dest="$1"
for p in hair03/hair03_ccby hair02/hair02_ccby hair01/hair01_cc0 shoes02/shoes02_ccby shoes01/shoes01_cc0 pants01/pants01_cc0 pants02/pants02_ccby shirts01/shirts01_cc0 shirts02/shirts02_ccby shirts03/shirts03_ccby dress01/dress01_cc0 skirts01/skirts01_cc0 skins01/skins01_cc0 skins02/skins02_cc0 bodyparts05/bodyparts05_cc0 bodyparts06/bodyparts06_cc-by eyebrows01/eyebrows01_cc0 eyelashes01/eyelashes01_cc0; do
  name=${p:t}
  dir=${p:h}
  mkdir -p "$dest/$dir"
  [[ -s "$dest/$dir/$name.zip" ]] || curl -sS -L -m 900 --retry 2 "https://files.makehumancommunity.org/asset_packs/$p.zip" -o "$dest/$dir/$name.zip"
  [[ -d "$dest/$dir/x" ]] || unzip -q -o "$dest/$dir/$name.zip" -d "$dest/$dir/x"
  echo "$name $(stat -f %z "$dest/$dir/$name.zip")"
done
echo done
