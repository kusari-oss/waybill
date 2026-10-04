# m1066 (#1052): two system configurations and no `packages` output. The
# NixOS one targets x86_64-linux, so evaluating it on any host also proves a
# configuration is evaluated for its own platform (FR-004). Never built.
{
  outputs = { self }: let
    mk = name: system: derivation {
      inherit name system;
      builder = "/bin/sh";
      args = [ "-c" "echo > $out" ];
    };
  in {
    darwinConfigurations.laptop.system = mk "darwin-system-laptop-1.0" "aarch64-darwin";
    nixosConfigurations.web01.config.system.build.toplevel = mk "nixos-system-web01-1.0" "x86_64-linux";
  };
}
