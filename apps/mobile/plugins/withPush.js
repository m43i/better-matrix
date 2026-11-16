// Push notifications' native set-up (ADR 0010, docs/push.md). Nothing secret lives here: the
// FCM and APNs credentials are in EAS (`eas credentials`), the gateway URL in the environment.
//
// - Runtime: `extra.push.appGroup` for the app (src/engine/paths.ts).
// - Android: `google-services.json` (Firebase, which Expo's push service delivers through) when
//   the file is there: the `GOOGLE_SERVICES_JSON` EAS file variable, else the `googleServicesFile`
//   option. Without it the app builds and runs, and push stays off.
// - iOS: the App Group (the engine's data, shared with the extension), a shared keychain access
//   group (the app's secrets, which expo-secure-store keeps in the default group: the first one
//   listed), and the Notification Service Extension target `BetterMatrixPush`,
//   which links the engine's static library (packages/matrix-native/build) and decrypts pushes.
//   EAS learns about the extension from `extra.eas.build.experimental.ios.appExtensions`.
//
//   `aps-environment` comes from expo-notifications' own plugin (its `mode` option).
//
// Options: { appGroup, googleServicesFile, notificationServiceExtension: boolean }.

const fs = require('node:fs');
const path = require('node:path');
const {
  withDangerousMod,
  withEntitlementsPlist,
  withXcodeProject,
} = require('expo/config-plugins');

const TARGET = 'BetterMatrixPush';
const SOURCES = path.join(__dirname, 'push', 'ios');
/** The engine's xcframework (`bun run ubrn:ios`), from the generated ios/ folder. */
const ENGINE = '$(PROJECT_DIR)/../../../packages/matrix-native/build/MatrixNativeFramework.xcframework';

function keychainGroup(bundleId) {
  return `$(AppIdentifierPrefix)${bundleId}`;
}

function addUnique(list, value) {
  const items = Array.isArray(list) ? [...list] : [];
  if (!items.includes(value)) items.push(value);
  return items;
}

/** Android: Firebase's config, when the owner provided it. */
function withGoogleServices(config, options) {
  const projectRoot = config._internal?.projectRoot ?? process.cwd();
  const candidates = [process.env.GOOGLE_SERVICES_JSON, options.googleServicesFile].filter(Boolean);
  const found = candidates.find((file) => fs.existsSync(path.resolve(projectRoot, file)));
  if (found) {
    config.android = { ...config.android, googleServicesFile: found };
  } else {
    console.warn('[withPush] no google-services.json: Android builds without push (docs/push.md)');
  }
  return config;
}

/** The app's own entitlements: the App Group and the shared keychain group. */
function withAppEntitlements(config, options) {
  const bundleId = config.ios?.bundleIdentifier;
  return withEntitlementsPlist(config, (config) => {
    const entitlements = config.modResults;
    entitlements['com.apple.security.application-groups'] = addUnique(
      entitlements['com.apple.security.application-groups'],
      options.appGroup,
    );
    // First: expo-secure-store writes to the first group, where the items were before too.
    const groups = (entitlements['keychain-access-groups'] ?? []).filter((g) => g !== keychainGroup(bundleId));
    entitlements['keychain-access-groups'] = [keychainGroup(bundleId), ...groups];
    return config;
  });
}

function infoPlist(config, options) {
  const version = config.version ?? '1.0.0';
  const build = config.ios?.buildNumber ?? '1';
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>$(DEVELOPMENT_LANGUAGE)</string>
  <key>CFBundleDisplayName</key>
  <string>${TARGET}</string>
  <key>CFBundleExecutable</key>
  <string>$(EXECUTABLE_NAME)</string>
  <key>CFBundleIdentifier</key>
  <string>$(PRODUCT_BUNDLE_IDENTIFIER)</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>$(PRODUCT_NAME)</string>
  <key>CFBundlePackageType</key>
  <string>$(PRODUCT_BUNDLE_PACKAGE_TYPE)</string>
  <key>CFBundleShortVersionString</key>
  <string>${version}</string>
  <key>CFBundleVersion</key>
  <string>${build}</string>
  <key>BMAppGroup</key>
  <string>${options.appGroup}</string>
  <key>BMKeychainGroup</key>
  <string>${keychainGroup(config.ios?.bundleIdentifier)}</string>
  <key>NSExtension</key>
  <dict>
    <key>NSExtensionPointIdentifier</key>
    <string>com.apple.usernotifications.service</string>
    <key>NSExtensionPrincipalClass</key>
    <string>$(PRODUCT_MODULE_NAME).NotificationService</string>
  </dict>
</dict>
</plist>
`;
}

function entitlementsPlist(config, options) {
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>com.apple.security.application-groups</key>
  <array>
    <string>${options.appGroup}</string>
  </array>
  <key>keychain-access-groups</key>
  <array>
    <string>${keychainGroup(config.ios?.bundleIdentifier)}</string>
  </array>
</dict>
</plist>
`;
}

/** The extension's files: the Swift, the C header, its Info.plist and entitlements. */
function withExtensionFiles(config, options) {
  return withDangerousMod(config, [
    'ios',
    (config) => {
      const dir = path.join(config.modRequest.platformProjectRoot, TARGET);
      fs.mkdirSync(dir, { recursive: true });
      for (const file of fs.readdirSync(SOURCES)) {
        fs.copyFileSync(path.join(SOURCES, file), path.join(dir, file));
      }
      fs.writeFileSync(path.join(dir, `${TARGET}-Info.plist`), infoPlist(config, options));
      fs.writeFileSync(path.join(dir, `${TARGET}.entitlements`), entitlementsPlist(config, options));
      return config;
    },
  ]);
}

/** The extension's target, embedded in the app. Adding it again changes nothing. */
function withExtensionTarget(config) {
  return withXcodeProject(config, (config) => {
    const project = config.modResults;
    if (project.pbxTargetByName(TARGET)) return config;
    const bundleId = `${config.ios?.bundleIdentifier}.${TARGET}`;
    const objects = project.hash.project.objects;
    // The xcode library expects these sections; a fresh Expo project lacks them.
    objects.PBXTargetDependency = objects.PBXTargetDependency ?? {};
    objects.PBXContainerItemProxy = objects.PBXContainerItemProxy ?? {};

    const files = [
      'NotificationService.swift',
      'BetterMatrixPush.h',
      'BetterMatrixPush-Bridging-Header.h',
      `${TARGET}-Info.plist`,
      `${TARGET}.entitlements`,
    ];
    const group = project.addPbxGroup(files, TARGET, TARGET);
    const mainGroup = project.getFirstProject().firstProject.mainGroup;
    project.addToPbxGroup(group.uuid, mainGroup);
    // The group made build files for every file; only the Swift is built.
    const buildFiles = objects.PBXBuildFile;
    for (const child of group.pbxGroup.children) {
      if (child.comment === 'NotificationService.swift') continue;
      for (const key of Object.keys(buildFiles)) {
        if (buildFiles[key]?.fileRef === child.value) {
          delete buildFiles[key];
          delete buildFiles[`${key}_comment`];
        }
      }
    }

    const target = project.addTarget(TARGET, 'app_extension', TARGET, bundleId);
    project.addBuildPhase(['NotificationService.swift'], 'PBXSourcesBuildPhase', 'Sources', target.uuid);
    project.addBuildPhase([], 'PBXResourcesBuildPhase', 'Resources', target.uuid);
    project.addBuildPhase([], 'PBXFrameworksBuildPhase', 'Frameworks', target.uuid);
    // The app embeds the extension right after its resources, before the script phases: after
    // them Xcode reports a dependency cycle.
    const app = project.getFirstTarget().firstTarget;
    const embed = app.buildPhases.findIndex((phase) => phase.comment === 'Copy Files');
    const resources = app.buildPhases.findIndex((phase) => phase.comment === 'Resources');
    if (embed > resources && resources >= 0) {
      const [phase] = app.buildPhases.splice(embed, 1);
      app.buildPhases.splice(resources + 1, 0, phase);
    }

    const deploymentTarget = '17.0';
    const sections = project.pbxXCBuildConfigurationSection();
    for (const key of Object.keys(sections)) {
      const settings = sections[key].buildSettings;
      if (!settings || settings.PRODUCT_NAME !== `"${TARGET}"`) continue;
      Object.assign(settings, {
        CLANG_ENABLE_MODULES: 'YES',
        CODE_SIGN_ENTITLEMENTS: `${TARGET}/${TARGET}.entitlements`,
        CODE_SIGN_STYLE: 'Automatic',
        INFOPLIST_FILE: `${TARGET}/${TARGET}-Info.plist`,
        IPHONEOS_DEPLOYMENT_TARGET: deploymentTarget,
        PRODUCT_BUNDLE_IDENTIFIER: `"${bundleId}"`,
        SWIFT_OBJC_BRIDGING_HEADER: `${TARGET}/${TARGET}-Bridging-Header.h`,
        SWIFT_VERSION: '5.0',
        TARGETED_DEVICE_FAMILY: '"1,2"',
        // The engine's static library, per platform slice of its xcframework.
        '"LIBRARY_SEARCH_PATHS[sdk=iphoneos*]"': `"${ENGINE}/ios-arm64"`,
        '"LIBRARY_SEARCH_PATHS[sdk=iphonesimulator*]"': `"${ENGINE}/ios-arm64-simulator"`,
        OTHER_LDFLAGS: [
          '"$(inherited)"',
          '"-lmatrix_ffi"',
          '"-lc++"',
          '"-lresolv"',
          '"-lz"',
          '"-framework"', '"Security"',
          '"-framework"', '"SystemConfiguration"',
          '"-framework"', '"CoreFoundation"',
          '"-framework"', '"Network"',
          '"-framework"', '"AVFoundation"',
          '"-framework"', '"AudioToolbox"',
          '"-framework"', '"CoreAudio"',
          '"-framework"', '"CoreMedia"',
          '"-framework"', '"CoreVideo"',
          '"-framework"', '"VideoToolbox"',
          '"-framework"', '"Metal"',
          '"-framework"', '"CoreGraphics"',
          '"-framework"', '"QuartzCore"',
        ],
        // The engine is big; only what the push path reaches is kept.
        DEAD_CODE_STRIPPING: 'YES',
      });
    }
    return config;
  });
}

/** Tells EAS about the extension, so its credentials (bundle ID, App Group) are made too. */
function withEasExtension(config, options) {
  const bundleIdentifier = `${config.ios?.bundleIdentifier}.${TARGET}`;
  const extra = config.extra ?? {};
  const eas = extra.eas ?? {};
  const build = eas.build ?? {};
  const experimental = build.experimental ?? {};
  const ios = experimental.ios ?? {};
  const others = (ios.appExtensions ?? []).filter((e) => e.targetName !== TARGET);
  config.extra = {
    ...extra,
    eas: {
      ...eas,
      build: {
        ...build,
        experimental: {
          ...experimental,
          ios: {
            ...ios,
            appExtensions: [
              ...others,
              {
                targetName: TARGET,
                bundleIdentifier,
                entitlements: { 'com.apple.security.application-groups': [options.appGroup] },
              },
            ],
          },
        },
      },
    },
  };
  return config;
}

module.exports = function withPush(config, input = {}) {
  const options = {
    appGroup: input.appGroup ?? `group.${config.ios?.bundleIdentifier}`,
    googleServicesFile: input.googleServicesFile ?? './google-services.json',
    notificationServiceExtension: input.notificationServiceExtension !== false,
  };
  config.extra = { ...config.extra, push: { ...config.extra?.push, appGroup: options.appGroup } };
  config = withGoogleServices(config, options);
  config = withAppEntitlements(config, options);
  if (options.notificationServiceExtension) {
    config = withEasExtension(config, options);
    config = withExtensionFiles(config, options);
    config = withExtensionTarget(config);
  }
  return config;
};
