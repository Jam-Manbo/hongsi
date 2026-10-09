<script lang="ts">
</script>

<div class="legend" aria-label="좌석 색">
  <span><i class="free"></i>빈 자리</span>
  <span><i class="used"></i>사용 중</span>
  <span><i class="recent"></i>방금 배정</span>
  <span><i class="mine"></i>내 자리</span>
  <span><i class="blocked"></i>이용 불가</span>
</div>

<style>
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    font-size: 12px;
    font-weight: 550;
    color: var(--text-2);
  }

  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    white-space: nowrap;
  }

  i {
    width: 12px;
    height: 12px;
    border-radius: 4px;
  }

  .free {
    background: var(--seat-free);
  }

  .used {
    background: var(--seat-used);
  }

  .recent {
    background: var(--warn-weak);
    box-shadow: inset 0 0 0 2px var(--warn);
  }

  .mine {
    background: var(--seat-mine);
  }

  .blocked {
    background: var(--seat-blocked);
    border: 1px solid var(--border);
  }
</style>
