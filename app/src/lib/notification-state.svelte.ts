import { emptyStatus, type NotificationIntent, type Permission } from './notification-model';
export const notificationState = $state({
  permission: 'unknown' as Permission,
  enabled: true,
  remote: false,
  due: emptyStatus(), seat: emptyStatus(),
  error: '',
  pending: null as NotificationIntent | null,
});
