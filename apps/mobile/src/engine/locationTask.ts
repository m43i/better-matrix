// Registered at bundle load so the system can deliver fixes while the screen is off.
import * as Location from 'expo-location';
import * as TaskManager from 'expo-task-manager';
import { t } from '../i18n';
import { geoUri } from '../state/geo';
import { LiveLocations } from './liveLocation';

const TASK = 'better-matrix-live-location';

export const liveLocations = new LiveLocations(async (wanted) => {
  const running = await Location.hasStartedLocationUpdatesAsync(TASK);

  if (wanted && !running) {
    await Location.startLocationUpdatesAsync(TASK, {
      accuracy: Location.Accuracy.High,
      timeInterval: 15000,
      distanceInterval: 10,
      showsBackgroundLocationIndicator: true,
      foregroundService: {
        notificationTitle: t('location.live'),
        notificationBody: t('location.tracking'),
        killServiceOnDestroy: true,
      },
    });
  } else if (!wanted && running) await Location.stopLocationUpdatesAsync(TASK);
});

TaskManager.defineTask<{ locations: Location.LocationObject[] }>(TASK, async ({ data, error }) => {
  const coords = data?.locations.at(-1)?.coords;

  if (coords || error)
    await liveLocations.deviceFix(
      coords && !error ? geoUri(coords.latitude, coords.longitude, coords.accuracy) : null,
    );
});
