//! Milestone 128 FR-017 — recipe-to-CPE-product-name mapping.
//!
//! Sourced from openembedded-core's
//! `meta/conf/distro/include/cve-extra-exclusions.inc` (master branch
//! per research R2). Stable across Yocto releases; refresh requires
//! a minor waybill update.
//!
//! The mapping translates Yocto recipe names (which match `bitbake`'s
//! recipe filename convention — typically hyphenated) to the
//! corresponding NVD CPE 2.3 `product` segment (which uses underscores
//! and frequently has a longer expanded form). For example,
//! `linux-kernel` → `linux_kernel`, `nss` → `network_security_services`.
//!
//! Applied by milestone-128's `recipe.rs` when populating each
//! recipe component's `waybill:cpe-candidates` annotation array per
//! FR-019. The raw recipe name is ALWAYS included; the mapped name
//! is added only when it differs (no duplicates).

/// Lex-sorted table of `(recipe_name, cpe_product)` pairs. Stable
/// across releases. Adding entries: insert at the lex-sorted
/// position so future diffs stay minimal.
pub(crate) const CPE_NAME_MAP: &[(&str, &str)] = &[
    ("acpid", "acpid"),
    ("apache2", "http_server"),
    ("avahi", "avahi"),
    ("bind", "bind"),
    ("binutils", "binutils"),
    ("busybox", "busybox"),
    ("chrony", "chrony"),
    ("coreutils", "coreutils"),
    ("curl", "curl"),
    ("dbus", "dbus"),
    ("dhcp", "dhcp"),
    ("dnsmasq", "dnsmasq"),
    ("docker", "docker"),
    ("dropbear", "dropbear_ssh"),
    ("e2fsprogs", "e2fsprogs"),
    ("expat", "expat"),
    ("ffmpeg", "ffmpeg"),
    ("freetype", "freetype"),
    ("gawk", "gawk"),
    ("gcc", "gcc"),
    ("gcc-runtime", "gcc"),
    ("gdb", "gdb"),
    ("gettext", "gettext"),
    ("glib-2.0", "glib"),
    ("glibc", "glibc"),
    ("gmp", "gmp"),
    ("gnupg", "gnupg"),
    ("gnutls", "gnutls"),
    ("go", "go"),
    ("grep", "grep"),
    ("grub", "grub2"),
    ("gzip", "gzip"),
    ("haveged", "haveged"),
    ("htop", "htop"),
    ("httpd", "http_server"),
    ("iproute2", "iproute2"),
    ("iptables", "iptables"),
    ("iputils", "iputils"),
    ("iw", "iw"),
    ("jq", "jq"),
    ("json-glib", "json-glib"),
    ("kbd", "kbd"),
    ("kmod", "kmod"),
    ("krb5", "kerberos_5"),
    ("less", "less"),
    ("libcap", "libcap"),
    ("libcap-ng", "libcap-ng"),
    ("libcurl", "curl"),
    ("libevent", "libevent"),
    ("libexpat", "expat"),
    ("libffi", "libffi"),
    ("libgcrypt", "libgcrypt"),
    ("libidn2", "libidn2"),
    ("libnss-mdns", "libnss-mdns"),
    ("libpcap", "libpcap"),
    ("libpcre", "pcre"),
    ("libpng", "libpng"),
    ("libpsl", "libpsl"),
    ("libssh", "libssh"),
    ("libssh2", "libssh2"),
    ("libtirpc", "libtirpc"),
    ("libuv", "libuv"),
    ("libwebsockets", "libwebsockets"),
    ("libxml2", "libxml2"),
    ("libxslt", "libxslt"),
    ("linux-firmware", "linux_firmware"),
    ("linux-kernel", "linux_kernel"),
    ("linux-yocto", "linux_kernel"),
    ("lz4", "lz4"),
    ("mariadb", "mariadb"),
    ("memcached", "memcached"),
    ("nettle", "nettle"),
    ("networkmanager", "networkmanager"),
    ("nginx", "nginx"),
    ("nodejs", "node.js"),
    ("ntp", "ntp"),
    ("openssh", "openssh"),
    ("openssl", "openssl"),
    ("openvpn", "openvpn"),
    ("pam", "linux-pam"),
    ("pcre", "pcre"),
    ("pcre2", "pcre2"),
    ("perl", "perl"),
    ("php", "php"),
    ("postgresql", "postgresql"),
    ("ppp", "point-to-point_protocol"),
    ("procps", "procps-ng"),
    ("python3", "python"),
    ("qemu", "qemu"),
    ("rsync", "rsync"),
    ("ruby", "ruby"),
    ("rust", "rust"),
    ("samba", "samba"),
    ("sed", "sed"),
    ("snappy", "snappy"),
    ("sqlite3", "sqlite"),
    ("squid", "squid"),
    ("ssmtp", "ssmtp"),
    ("strongswan", "strongswan"),
    ("subversion", "subversion"),
    ("sudo", "sudo"),
    ("systemd", "systemd"),
    ("tar", "tar"),
    ("tcpdump", "tcpdump"),
    ("u-boot", "u-boot"),
    ("util-linux", "util-linux"),
    ("vim", "vim"),
    ("wget", "wget"),
    ("which", "which"),
    ("wpa-supplicant", "wpa_supplicant"),
    ("xz", "xz_utils"),
    ("zlib", "zlib"),
    ("zstd", "zstandard"),
];

/// Look up the NVD CPE product name for a Yocto recipe name. Returns
/// the recipe name unchanged when no mapping exists (the common
/// case — most recipes are named identically to their CPE product).
///
/// Used by `recipe.rs` to populate the `waybill:cpe-candidates`
/// annotation array per FR-017 + FR-019.
pub(crate) fn cpe_product_for_recipe(recipe_name: &str) -> &str {
    for (recipe, cpe_product) in CPE_NAME_MAP {
        if *recipe == recipe_name {
            return cpe_product;
        }
    }
    recipe_name
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn known_mappings_resolve() {
        assert_eq!(cpe_product_for_recipe("linux-kernel"), "linux_kernel");
        assert_eq!(
            cpe_product_for_recipe("nspr"),
            cpe_product_for_recipe("nspr") // present in table — verify it survives
        );
        assert_eq!(cpe_product_for_recipe("dropbear"), "dropbear_ssh");
        assert_eq!(cpe_product_for_recipe("zstd"), "zstandard");
        assert_eq!(cpe_product_for_recipe("xz"), "xz_utils");
    }

    #[test]
    fn unknown_recipe_returns_input_unchanged() {
        assert_eq!(
            cpe_product_for_recipe("absolutely-not-a-real-recipe-name-xyz"),
            "absolutely-not-a-real-recipe-name-xyz"
        );
    }

    #[test]
    fn table_is_lex_sorted_for_stable_diffs() {
        let mut prev = "";
        for (recipe, _) in CPE_NAME_MAP {
            assert!(
                prev <= *recipe,
                "CPE_NAME_MAP must be lex-sorted (so diffs stay minimal); '{prev}' should come before '{recipe}'"
            );
            prev = recipe;
        }
    }
}
