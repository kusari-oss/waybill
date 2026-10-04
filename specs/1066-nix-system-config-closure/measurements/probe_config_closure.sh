#!/usr/bin/env bash
# #1052 / m1066 probe: cost of a system configuration's build closure, the way
# the closure tier reads it (evaluate-only `nix derivation show -r`, IFD
# refused). Builds a throwaway flake with one minimal NixOS (x86_64-linux) and
# one minimal nix-darwin configuration, locks it, then measures each twice
# (cold, warm). Needs network for the lock. Usage: probe_config_closure.sh
set -u
W=$(mktemp -d); cd "$W"
cat > flake.nix <<'NIX'
{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
  inputs.nix-darwin.url = "github:nix-darwin/nix-darwin/nix-darwin-25.05";
  inputs.nix-darwin.inputs.nixpkgs.follows = "nixpkgs";
  outputs = { self, nixpkgs, nix-darwin }: {
    nixosConfigurations.web01 = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [ { boot.loader.grub.enable = false; fileSystems."/".device = "/dev/sda1"; system.stateVersion = "25.05"; } ];
    };
    darwinConfigurations.laptop = nix-darwin.lib.darwinSystem {
      system = "aarch64-darwin";
      modules = [ { system.stateVersion = 6; } ];
    };
  };
}
NIX
git init -q && git add flake.nix
/usr/bin/time -p nix flake lock 2>&1 | grep '^real' | sed 's/^/lock /'
git add flake.lock && git -c user.email=p@p -c user.name=p commit -qm init
m() {
  label=$1; shift
  /usr/bin/time -p nix derivation show -r --option allow-import-from-derivation false "$@" > out.json 2> err.txt
  t=$(grep '^real' err.txt | awk '{print $2}')
  n=$(python3 -c "import json; d=json.load(open('out.json')); d=d.get('derivations',d); print(len(d))" 2>/dev/null || echo ERR)
  echo "$label drv=$n real=${t}s"
}
m "nixos  cold" '.#nixosConfigurations.web01.config.system.build.toplevel'
m "nixos  warm" '.#nixosConfigurations.web01.config.system.build.toplevel'
m "darwin cold" '.#darwinConfigurations.laptop.system'
m "darwin warm" '.#darwinConfigurations.laptop.system'
rm -rf "$W"
