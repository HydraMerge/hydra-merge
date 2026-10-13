macOS archives contain HydraMerge.app. Linux and Windows archives contain the hydra-merge binary. x86_64 and arm64.

**macOS**: the build is not notarized. After moving HydraMerge.app to /Applications, clear the quarantine flag once, otherwise macOS refuses to open it:

```
xattr -dr com.apple.quarantine /Applications/HydraMerge.app
```

Updates installed from inside the app clear the flag themselves.
