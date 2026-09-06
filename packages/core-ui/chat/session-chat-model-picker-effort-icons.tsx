import './session-chat-model-picker-effort-icons.css';

/** A growing constellation: spark, orbit, star, nova, reactor, then a solar vortex. */
export function ModelPickerEffortIcon({ effort }: { effort: string }) {
  const artwork =
    effort === 'low' ? (
      <>
        <circle cx='28' cy='28' r='14' opacity='.25' strokeDasharray='1 5' />
        <path d='m28 17 2.8 8.2L39 28l-8.2 2.8L28 39l-2.8-8.2L17 28l8.2-2.8Z' />
        <circle cx='28' cy='28' r='2' fill='currentColor' stroke='none' />
      </>
    ) : effort === 'medium' ? (
      <>
        <ellipse cx='28' cy='28' rx='19' ry='9' transform='rotate(-35 28 28)' />
        <ellipse cx='28' cy='28' rx='10' ry='18' transform='rotate(-35 28 28)' opacity='.4' />
        <circle cx='28' cy='28' r='4' fill='currentColor' fillOpacity='.15' />
        <circle cx='43' cy='17' r='2.2' fill='currentColor' stroke='none' />
      </>
    ) : effort === 'high' ? (
      <>
        <path d='m28 7 4.5 16.5L49 28l-16.5 4.5L28 49l-4.5-16.5L7 28l16.5-4.5Z' />
        <path d='m15 15 5 5m16 16 5 5m0-26-5 5M20 36l-5 5' opacity='.45' />
        <circle cx='28' cy='28' r='4' />
      </>
    ) : effort === 'xhigh' ? (
      <>
        <path d='m28 5 5 13 11-6-6 11 13 5-13 5 6 11-11-6-5 13-5-13-11 6 6-11L5 28l13-5-6-11 11 6Z' />
        <path d='m28 16 12 12-12 12-12-12Z' opacity='.4' />
        <circle cx='28' cy='28' r='3' fill='currentColor' stroke='none' />
      </>
    ) : effort === 'max' ? (
      <>
        <g className='effort-icon-orbit'>
          <circle cx='28' cy='28' r='22' strokeDasharray='23 12' opacity='.6' />
          <circle cx='28' cy='6' r='2.2' fill='currentColor' stroke='none' />
          <circle cx='28' cy='50' r='2.2' fill='currentColor' stroke='none' />
        </g>
        <g className='effort-icon-core'>
          <path d='m28 11 15 8.5v17L28 45l-15-8.5v-17Z' />
          <path d='m28 17 3.5 7.5L39 28l-7.5 3.5L28 39l-3.5-7.5L17 28l7.5-3.5Z' fill='currentColor' fillOpacity='.15' />
        </g>
      </>
    ) : (
      <>
        <g className='effort-icon-orbit'>
          <path
            d='M28 3a25 25 0 0 1 23 16M53 28a25 25 0 0 1-16 23M28 53A25 25 0 0 1 5 37M3 28A25 25 0 0 1 19 5'
            opacity='.55'
          />
          <circle cx='28' cy='3' r='1.8' fill='currentColor' stroke='none' />
          <circle cx='28' cy='53' r='1.8' fill='currentColor' stroke='none' />
        </g>
        <g className='effort-icon-counter-orbit'>
          <ellipse cx='28' cy='28' rx='21' ry='12' transform='rotate(45 28 28)' opacity='.75' />
          <ellipse cx='28' cy='28' rx='21' ry='12' transform='rotate(-45 28 28)' opacity='.4' />
        </g>
        <g className='effort-icon-core'>
          <path
            d='m28 9 4 13 13-5-11 11 13 4-15 1-4 14-4-14-15-1 13-4-11-11 13 5Z'
            fill='currentColor'
            fillOpacity='.12'
          />
          <circle cx='28' cy='28' r='3.5' fill='currentColor' stroke='none' />
        </g>
      </>
    );
  return (
    <svg
      className='model-picker-effort-icon'
      aria-hidden='true'
      viewBox='0 0 56 56'
      fill='none'
      stroke='currentColor'
      strokeWidth='1.25'
      strokeLinecap='round'
      strokeLinejoin='round'
    >
      {artwork}
    </svg>
  );
}
