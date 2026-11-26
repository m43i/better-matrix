import { coordinates, coordinatesText, geoUri, mapOf, navigationLink } from '../geo';

// The engine's `geo.rs` reads and writes the same (its doc tests).
describe('coordinates', () => {
  it('should read a geo URI with an accuracy', () => {
    expect(coordinates('geo:51.5008,0.1247;u=35')).toEqual({
      latitude: 51.5008,
      longitude: 0.1247,
    });
  });

  it('should read nothing from another address', () => {
    expect(coordinates('https://example.org')).toBeNull();
  });

  it('should read nothing in another coordinate system', () => {
    expect(coordinates('geo:1,2;crs=mars')).toBeNull();
  });

  it('should read nothing out of range', () => {
    expect(coordinates('geo:91,0')).toBeNull();
  });
});

describe('geoUri', () => {
  it('should write six decimals without trailing zeros', () => {
    expect(geoUri(52.516275, 13.3777)).toBe('geo:52.516275,13.3777');
  });

  it('should add how accurate the position is', () => {
    expect(geoUri(1, 2, 12.4)).toBe('geo:1,2;u=12');
  });
});

describe('mapOf', () => {
  it('should name the map the engine draws', () => {
    expect(mapOf('geo:52.5163,13.3777;u=10')).toBe('map:52.5163,13.3777,15,light');
  });
});

describe('navigationLink', () => {
  it('should address the configured iOS navigation app', () => {
    expect(navigationLink('geo:52.5,13.4', 'ios')).toBe(
      'geo-navigation:///place?coordinate=52.5,13.4',
    );
  });
  it('should open the Android navigation handler with only validated coordinates', () => {
    expect(navigationLink('geo:52.5,13.4;u=10', 'android')).toBe('geo:0,0?q=52.5,13.4');
    expect(navigationLink('https://example.org', 'android')).toBeNull();
  });
});

describe('coordinatesText', () => {
  it('should show the coordinates as people read them', () => {
    expect(coordinatesText('geo:-0.0000001,2')).toBe('0, 2');
  });
});
