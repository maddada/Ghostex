import type { Meta, StoryObj } from '@storybook/react-vite';
import { IconInfoCircle } from '@tabler/icons-react';
import { useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import { Input } from '@/packages/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/packages/components/ui/popover';
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';

function ThemeBoundaries() {
  const [choice, setChoice] = useState('first');
  return (
    <main className='mx-auto grid w-full max-w-4xl gap-8 p-6 text-foreground'>
      <header>
        <h1 className='text-xl font-semibold'>Shared theme behavior</h1>
        <p className='mt-2'>This page scrolls normally. Its text can be selected and copied.</p>
      </header>
      <section
        className='ghostex-settings-shadcn dark grid gap-4 rounded-xl bg-background p-5'
        aria-label='Settings controls'
      >
        <h2 className='font-medium'>Settings controls</h2>
        <div className='flex flex-wrap items-center gap-4'>
          <Button variant='outline' className='rounded-none [&_svg]:size-4'>
            <IconInfoCircle /> Flat corners
          </Button>
          <div className='size-10 bg-foreground/20' aria-label='Square size utility' />
          <div className='size-10 rounded-full bg-foreground/20' aria-label='Round size utility' />
          <Input className='w-40' aria-label='Default input corners' placeholder='Default input corners' />
          <Input className='w-40 rounded-none' aria-label='Flat input corners' placeholder='Flat input corners' />
        </div>
        <SegmentedControl value={choice} onValueChange={setChoice}>
          <SegmentedControlItem value='first'>First</SegmentedControlItem>
          <SegmentedControlItem value='second'>Second</SegmentedControlItem>
        </SegmentedControl>
      </section>
      <section className='grid gap-4' aria-label='Component styles'>
        <h2 className='font-medium'>Component styles</h2>
        <div className='flex flex-wrap items-center gap-4'>
          <button
            type='button'
            className='rounded-none border-0 bg-[#2563eb] px-3 py-2 text-xs font-semibold text-white transition-transform hover:scale-105'
          >
            Custom plain button
          </button>
          <Popover>
            <PopoverTrigger render={<Button variant='outline' />}>Open custom popup</PopoverTrigger>
            <PopoverContent style={{ background: '#1e3a8a', border: '2px solid #60a5fa', color: '#fff' }}>
              <p>This popup owns its blue background, border and text color.</p>
            </PopoverContent>
          </Popover>
          <Popover>
            <PopoverTrigger render={<Button variant='outline' />}>Open standard popup</PopoverTrigger>
            <PopoverContent>
              <p>This popup follows the shared palette.</p>
            </PopoverContent>
          </Popover>
        </div>
      </section>
      <section className='grid gap-4' aria-label='Independent scrolling'>
        <h2 className='font-medium'>Independent scrolling</h2>
        <div className='h-48 overflow-y-auto rounded-lg border p-4' tabIndex={0} aria-label='Scrollable content'>
          {Array.from({ length: 16 }, (_, index) => (
            <p className='py-3' key={index}>
              Scrollable row {index + 1}
            </p>
          ))}
          <p>End of scrollable content</p>
        </div>
        <p>This paragraph is ordinary selectable page content.</p>
      </section>
      <footer className='border-t py-6'>End of theme preview</footer>
    </main>
  );
}

export default {
  title: 'Components/Theme Boundaries',
  component: ThemeBoundaries,
  parameters: { layout: 'fullscreen' },
} satisfies Meta<typeof ThemeBoundaries>;
type Story = StoryObj<typeof ThemeBoundaries>;
export const Default: Story = {};
