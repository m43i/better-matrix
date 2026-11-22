import { coordinates, coordinatesText, geoUri, mapOf, webLink } from '../geo';

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
    expect(mapOf('geo:52.5163,13.3777;u=10')).toBe('osm:52.5163,13.3777,15');
  });
});

describe('webLink', () => {
  it('should link the place on openstreetmap.org', () => {
    expect(webLink('geo:52.5163,13.3777')).toBe(
      'https://www.openstreetmap.org/?mlat=52.5163&mlon=13.3777#map=17/52.5163/13.3777',
    );
  });
});

describe('coordinatesText', () => {
  it('should show the coordinates as people read them', () => {
    expect(coordinatesText('geo:-0.0000001,2')).toBe('0, 2');
  });
});
