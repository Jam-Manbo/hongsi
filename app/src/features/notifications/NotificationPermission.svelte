<script lang="ts">
  import { acceptNotificationPermission, cancelNotificationPermission, notificationPermission, notificationState } from './notify';
  import Sheet from '../../shared/ui/Sheet.svelte';
  import { isApp } from '../../shared/api/api';
</script>

<Sheet bind:open={notificationPermission.open} title={notificationState.permission === 'denied' ? '알림 권한을 확인해 주세요.' : '알림을 허용해 주세요.'} titleIcon="bell" confirm onclose={cancelNotificationPermission}>
  <div class="permission-info">
    <p>새 알림과 마감 시간을 알려드리려면 <span class="keep">알림 권한</span>이 필요해요.</p>
    <p class="hint">{#if notificationState.permission === 'denied'}{isApp ? '기기 설정 → 알림에서 홍시의 알림을 허용한 뒤 다시 확인해 주세요.' : '브라우저의 사이트 설정에서 홍시 알림을 허용한 뒤 다시 확인해 주세요.'}{:else}알림 권한을 허용해 주세요.{/if}</p>
  </div>
  {#snippet footer()}
    <button class="btn btn-ghost w1" onclick={cancelNotificationPermission}>나중에</button>
    <button class="btn btn-primary w2" disabled={notificationPermission.busy} onclick={acceptNotificationPermission}>{notificationPermission.busy ? '권한 확인 중…' : notificationState.permission === 'denied' ? '권한 다시 확인' : '알림 허용'}</button>
  {/snippet}
</Sheet>

<style>
  .permission-info { display: grid; gap: 14px; }
  p { font-size: 15px; line-height: 1.7; color: var(--text); }
  .hint { font-size: 13px; color: var(--text-2); }
  .keep { white-space: nowrap; }
</style>
