# macOS Gatekeeper, signing, and notarization

## Open the current unsigned release

The macOS `.app.zip` and `.dmg` in `v0.1.0` are **unsigned and not notarized**. The released bundle has an `Info.plist` and executable, but no `Contents/_CodeSignature`; that is why macOS can report it as “damaged” after download. Only use this workaround for an asset from the official Valkey Manager release that you trust.

1. Open the DMG and drag **Valkey Manager.app** to `/Applications`.
2. Remove the download quarantine for this app only, then launch it:

   ```sh
   xattr -dr com.apple.quarantine "/Applications/Valkey Manager.app"
   open "/Applications/Valkey Manager.app"
   ```

If the app is still inside the mounted DMG, use its mounted path instead, for example `/Volumes/Valkey Manager/Valkey Manager.app`. This does not disable Gatekeeper globally and does not make the app signed.

## Prepare proper Developer ID signing

For a normal download-and-open experience, build a signed app and notarize it with Apple. The release workflow should use an active Apple Developer Program membership and:

- A **Developer ID Application** certificate, exported from Keychain Access as a password-protected `.p12` with its private key.
- An App Store Connect API key and its Key ID, Issuer ID, and `.p8` private key.
- The Apple Developer Team ID associated with those credentials.

Do not send certificates, private keys, passwords, or API keys in chat or commit them to the repository. Configure them as repository Actions secrets. Example names:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE_P12_BASE64` | Base64-encoded Developer ID `.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | Password used to export the `.p12` |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Name (TEAMID)` |
| `APPLE_TEAM_ID` | Apple Developer Team ID |
| `APPLE_KEYCHAIN_PASSWORD` | Random password for the temporary CI keychain |
| `APPLE_NOTARY_API_KEY_BASE64` | Base64-encoded App Store Connect `.p8` key |
| `APPLE_NOTARY_KEY_ID` | App Store Connect API Key ID |
| `APPLE_NOTARY_ISSUER_ID` | App Store Connect Issuer ID |

For example, encode the files locally and paste the output directly into the GitHub secret UI:

```sh
base64 < DeveloperID.p12 | tr -d '\n'
base64 < AuthKey_ABC123.p8 | tr -d '\n'
```

Never enable signing secrets for untrusted pull-request builds. Restrict secret use to tag releases and a maintainer-triggered release workflow.

In the macOS release job, decode and import the certificate into an ephemeral keychain. A `RUNNER_TEMP` keychain password should be generated for each run or supplied through `APPLE_KEYCHAIN_PASSWORD`:

```sh
KEYCHAIN_PATH="$RUNNER_TEMP/valkey-signing.keychain-db"
CERTIFICATE_PATH="$RUNNER_TEMP/developer-id.p12"

python3 - "$CERTIFICATE_PATH" <<'PY'
import base64
import os
import sys
from pathlib import Path

Path(sys.argv[1]).write_bytes(base64.b64decode(os.environ["APPLE_CERTIFICATE_P12_BASE64"]))
PY

security create-keychain -p "$APPLE_KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH"
security set-keychain-settings -lut 21600 "$KEYCHAIN_PATH"
security unlock-keychain -p "$APPLE_KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH"
security import "$CERTIFICATE_PATH" -k "$KEYCHAIN_PATH" \
  -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple: -s \
  -k "$APPLE_KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH"
security list-keychains -d user -s "$KEYCHAIN_PATH"
security default-keychain -d user -s "$KEYCHAIN_PATH"

NOTARY_KEY_PATH="$RUNNER_TEMP/asc-notary-key.p8"
python3 - "$NOTARY_KEY_PATH" <<'PY'
import base64
import os
import sys
from pathlib import Path

Path(sys.argv[1]).write_bytes(base64.b64decode(os.environ["APPLE_NOTARY_API_KEY_BASE64"]))
PY
xcrun notarytool store-credentials valkey-notary \
  --key "$NOTARY_KEY_PATH" \
  --key-id "$APPLE_NOTARY_KEY_ID" \
  --issuer "$APPLE_NOTARY_ISSUER_ID"
```

## Release-job procedure

The macOS release job needs to split `scripts/package-macos.sh` into bundle creation and final packaging, then run signing/notarization before uploading assets:

1. Create and unlock a temporary keychain; import the `.p12`; set its key partition list so `codesign` can use the private key. Keep the keychain password in `APPLE_KEYCHAIN_PASSWORD` and delete the temporary keychain and certificate files on both success and failure.
2. Sign `Valkey Manager.app` with Developer ID, hardened runtime, and a secure timestamp:

   ```sh
   codesign --force --options runtime --timestamp \
     --sign "$APPLE_SIGNING_IDENTITY" "$APP_BUNDLE"
   codesign --verify --deep --strict --verbose=2 "$APP_BUNDLE"
   ```

3. Create a DMG containing the signed app and submit it to Apple with the App Store Connect API key stored in the temporary keychain. The executables in the app must be signed; the DMG itself is the notarization container:

   ```sh
   xcrun notarytool submit "$DMG" \
      --keychain-profile valkey-notary --wait
   ```

4. After notarization is accepted, staple and validate the ticket on both the app and DMG. Recreate the `.app.zip` from the stapled app because a ticket cannot be stapled directly to a ZIP:

   ```sh
   xcrun stapler staple "$APP_BUNDLE"
   xcrun stapler validate "$APP_BUNDLE"
   # Recreate the .app.zip from the stapled bundle here.
   xcrun stapler staple "$DMG"
   xcrun stapler validate "$DMG"
   spctl --assess --type execute --verbose=4 "$APP_BUNDLE"
   ```

Apple does not allow stapling a ticket directly to a ZIP; staple the `.app` first and then recreate the archive. See [Apple: Notarizing macOS software](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution) and [Customizing the notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow).

The current `scripts/package-macos.sh` only creates the bundle and installers. Do not describe a release as signed/notarized until the signature, notarization, and stapling checks above pass. The current GitHub Actions configuration does not yet consume the Apple secrets; add the signing/notarization steps before enabling them.
