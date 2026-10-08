import { isIOS } from './env';

export async function currentLocation(): Promise<{ latitude: number; longitude: number; accuracy: number }> {
  if (isIOS) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke('current_location');
  }
  if (!('geolocation' in navigator)) throw new Error('이 기기에서는 위치를 확인할 수 없어요.');
  return new Promise((resolve, reject) => navigator.geolocation.getCurrentPosition(
    ({ coords }) => resolve({ latitude: coords.latitude, longitude: coords.longitude, accuracy: coords.accuracy }),
    (error) => reject(new Error(error.code === 1 ? '위치 권한을 허용해 주세요.' : '위치를 찾지 못했어요.')),
    { enableHighAccuracy: true, timeout: 12_000, maximumAge: 30_000 },
  ));
}
