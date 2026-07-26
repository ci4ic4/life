# Deploying to o2

Goal: `https://ci4o2.zapto.org/life/` serves the checkout at `/home/ci4/src/life`
directly, so publishing is `git pull` and nothing else — no copying files into the
nginx web root.

## One-time setup

Run these **on o2**, not here.

**1. Let nginx traverse into the checkout.** Home directories are usually `700`, which
stops the nginx worker before it ever reaches the repo. Grant traverse (`x`) only — this
does not make the directories listable:

```sh
chmod o+x /home/ci4 /home/ci4/src
find /home/ci4/src/life -type d -exec chmod o+rx {} +
find /home/ci4/src/life -type f -exec chmod o+r {} +
```

**2. Wire up the location blocks.**

```sh
sudo nginx -T | grep -n 'server_name ci4o2'      # find the right server block / file
```

Then add one line inside that `server { ... }` block:

```nginx
include /home/ci4/src/life/deploy/nginx-life.conf;
```

**3. Reload.**

```sh
sudo nginx -t && sudo systemctl reload nginx
```

**4. SELinux — Oracle Linux / RHEL only.** OCI images ship it enforcing, and it will
deny nginx access to anything under `/home` with a `permission denied` in the error log
even when the file mode is right. Check and fix:

```sh
getenforce                                  # "Enforcing" means this step applies
sudo setsebool -P httpd_read_user_content 1
```

If that is not enough, label the tree for the web server instead:

```sh
sudo semanage fcontext -a -t httpd_sys_content_t "/home/ci4/src/life(/.*)?"
sudo restorecon -Rv /home/ci4/src/life
```

Debian/Ubuntu has no SELinux by default — skip this.

## Publishing, from then on

```sh
ssh o2 'cd /home/ci4/src/life && git pull --ff-only'
```

New simulations appear automatically once they are linked from `index.html`.

## Checks

```sh
curl -sI  https://ci4o2.zapto.org/life/            | head -1   # 200
curl -sI  https://ci4o2.zapto.org/life/.git/config | head -1   # 404, not 200
curl -s   https://ci4o2.zapto.org/life/ | grep -c 'class="card"'  # link count
```

The second one matters. A checkout served as a web root hands out `.git` unless something
blocks it; `nginx-life.conf` does, and that command is how you confirm it.

## Notes

- The old `/life-ecology.html` path at the site root still works if anything links to it —
  this adds a location, it does not remove one.
- The pages pull three.js from jsDelivr, so a client with no internet gets a blank canvas
  on the torus simulations. `life-conveyor.html` is plain 2D canvas and has no CDN
  dependency.
- `Cache-Control: no-cache` means revalidate, not "never cache" — nginx still answers 304
  from the ETag, so a repeat visit transfers almost nothing.
