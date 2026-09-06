/** Line artwork for the Electric Axis model tiles. */
export function ModelPickerIcon({ model }: { model: string }) {
  const artwork = model.includes('astra') ? (
    <>
      <ellipse cx='24' cy='24' rx='9' ry='21' transform='rotate(35 24 24)' />
      <ellipse cx='24' cy='24' rx='9' ry='21' transform='rotate(-35 24 24)' />
      <ellipse cx='24' cy='24' rx='9' ry='21' transform='rotate(90 24 24)' />
      <circle cx='24' cy='24' r='2.3' fill='currentColor' stroke='none' />
    </>
  ) : model.includes('sol') ? (
    <>
      <circle cx='24' cy='24' r='9' />
      {Array.from({ length: 12 }, (_, index) => (
        <path key={index} d={index % 2 ? 'M24 6v4' : 'M24 3v7'} transform={`rotate(${index * 30} 24 24)`} />
      ))}
      <circle cx='24' cy='24' r='5.5' opacity='.18' />
    </>
  ) : model.includes('terra') ? (
    <>
      <path d='m3 37 12-23 8 13 7-20 15 30H3Z' />
      <path d='m11 22 4 5 4-4m7-5 4 5 4-5M17 37l6-10 6 10' opacity='.65' />
    </>
  ) : model.includes('luna') ? (
    <>
      <path d='M29 5a19 19 0 1 0 13 28A20 20 0 0 1 29 5Z' strokeWidth='1.35' />
    </>
  ) : model === 'fable' ? (
    <>
      <path d='M24 13C17 7 9 9 4 11v27c8-3 14-2 20 2 6-4 12-5 20-2V11c-5-2-13-4-20 2v27' />
      <path d='m10 17 8 2m-8 5 8 2m12-7 8-2m-8 9 8-2' opacity='.5' />
    </>
  ) : model === 'opus[1m]' ? (
    <>
      <path d='M24 24c-6-13-20-13-20 0s14 13 20 0 20-13 20 0-14 13-20 0Z' />
      <circle cx='24' cy='24' r='20' opacity='.25' />
    </>
  ) : model === 'opus' ? (
    <>
      {Array.from({ length: 12 }, (_, i) => (
        <path key={i} d='M24 5v12' transform={`rotate(${i * 30} 24 24)`} />
      ))}
      <circle cx='24' cy='24' r='5' />
    </>
  ) : model === 'sonnet' ? (
    <>
      <path d='M12 8c-6 1-7 7-3 12l5 6v8c0 5 4 8 10 8s10-3 10-8v-8l5-6c4-5 3-11-3-12' />
      <path d='M12 8c3 4 7 6 12 6s9-2 12-6M15 34h18M15 42h18' />
      <path d='M18 14v19m6-18v18m6-19v19' strokeWidth='1.2' opacity='.75' />
      <circle cx='11' cy='8' r='2' />
      <circle cx='37' cy='8' r='2' />
    </>
  ) : (
    <>
      <path d='M8 38C2 18 16 5 42 7c-1 25-14 40-34 31Zm0 0L32 16M18 28l-2-10m11 1 9 1' />
    </>
  );
  return (
    <svg
      aria-hidden='true'
      viewBox='0 0 48 48'
      fill='none'
      stroke='currentColor'
      strokeWidth='1.6'
      strokeLinecap='round'
      strokeLinejoin='round'
    >
      {artwork}
    </svg>
  );
}
