# Deploying to o2

`https://ci4o2.zapto.org/life/` serves the checkout at `/home/ci4/src/life` directly, so
publishing is `git pull` and nothing else — no copying files into the nginx web root.

## The host

o2 runs **NetBSD-current** with nginx from pkgsrc. That decides every path below, and none
of it looks like a Linux install. (Exact OS and nginx versions are deliberately left out —
this file is public on GitHub, and a precise version plus architecture is the filter a
vulnerability scanner wants. Run `uname -a` and `/usr/pkg/sbin/nginx -v` on the host.)

| | |
|---|---|
| nginx binary | `/usr/pkg/sbin/nginx` (not on the login `PATH`) |
| config | `/usr/pkg/etc/nginx/nginx.conf` |
| this snippet | `/usr/pkg/etc/nginx/life.conf` |
| worker user | `nginx` |
| service | `/etc/rc.d/nginx reload`, enabled by `nginx=YES` in `/etc/rc.conf` |
| site root | `share/examples/nginx/html`, relative to the `--prefix`, so `/usr/pkg/share/examples/nginx/html` |
| TLS | `/etc/ssl/acme/ci4o2.zapto.org.fullchain.pem`, ACME served over :80 from `/var/www/acme/` |

No SELinux, no AppArmor — nothing to relabel.

## Setup — already done

**1. Permissions.** Nothing was needed. `/home`, `/home/ci4`, `/home/ci4/src` and the
checkout are all `drwxr-xr-x`, so the `nginx` worker can already traverse and read. Confirm
with:

```sh
sudo -u nginx test -r /home/ci4/src/life/life-torus.html && echo readable
```

**2. The snippet is installed as a host file**, copied from `deploy/nginx-life.conf`:

```sh
sudo install -o root -g wheel -m 644 nginx-life.conf /usr/pkg/etc/nginx/life.conf
```

Deliberately *not* `include`-ing the repo copy directly. nginx refuses to start if an
`include` target is missing, so pointing it into a user's git checkout means moving or
removing that checkout takes the whole web server down with it — including the unrelated
sites. The repo copy is the reference; the host keeps its own.

**3. Included** in the `:443` server block of `nginx.conf`, after `location / {}`:

```nginx
include /usr/pkg/etc/nginx/life.conf;
```

Original saved alongside as `nginx.conf.bak-20260726`.

**4. Applied:**

```sh
sudo /usr/pkg/sbin/nginx -t && sudo /etc/rc.d/nginx reload
```

## Publishing, from now on

```sh
ssh o2 'cd /home/ci4/src/life && git pull --ff-only'
```

New simulations appear as soon as they are linked from `index.html`. Nothing else to copy.

## Checks

Verified live on 2026-07-26:

```
/life/life-torus.html   200      served straight out of the checkout
/life/life-stats.js     200      Content-Encoding: gzip, Cache-Control: no-cache
/life/.git/config       404      denied
/life/target/           404      denied
/life/docs/             404      denied
/life-ecology.html      200      the old root-level copy still works
```

`/life/` itself answers **403** until the checkout actually contains `index.html`
(`directory index ... is forbidden`, with `autoindex off`). That is the correct failure —
it means the location is live and the content is merely behind. It turns 200 on the first
pull that brings `index.html` in.

The `.git` check is the one worth keeping. Serving a git checkout as a web root hands the
full repository to anyone who asks; the deny block in `nginx-life.conf` stops it, and that
curl is how you know it still does.
