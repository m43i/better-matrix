// Android: the size of the bottom bar's badge dot (the account tab's "something new"). Material
// draws a textless badge 6 dp wide, too small next to the bar's icons; its text badge is 16 dp,
// too large. react-native-screens wraps the bar in its own Material 3 theme, so the app theme's
// badge style never reaches it; instead the app overrides Material's dot size resource
// (`m3_badge_size`, which only the textless badge uses), as apps may override a library's
// resources. The dot stays Material's own.

const fs = require('node:fs');

const path = require('node:path');

const { withDangerousMod } = require('expo/config-plugins');

const DOT = '10dp';

module.exports = function withBadgeSize(config) {
  return withDangerousMod(config, [
    'android',
    (config) => {
      const values = path.join(config.modRequest.platformProjectRoot, 'app/src/main/res/values');
      fs.mkdirSync(values, { recursive: true });
      fs.writeFileSync(
        path.join(values, 'badge_dimens.xml'),
        `<?xml version="1.0" encoding="utf-8"?>
<!-- Written by plugins/withBadgeSize.js: the bottom bar's badge dot. -->
<resources xmlns:tools="http://schemas.android.com/tools">
  <dimen name="m3_badge_size" tools:override="true">${DOT}</dimen>
</resources>
`,
      );

      return config;
    },
  ]);
};
