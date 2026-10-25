import { Component, HostBinding, HostListener, computed, input, output } from '@angular/core';
import { NgIf, NgTemplateOutlet } from '@angular/common';
import { Child } from './child';
import { Hint } from './hint';

@Component({
  selector: 'x-panel',
  standalone: true,
  imports: [Child, Hint, NgIf, NgTemplateOutlet],
  host: { '[attr.data-hint]': 'variant', role: 'region', '(keydown)': 'onKey($event)' },
  template: `
    <ng-template #menu let-item><a data-hint="pick">{{ item }}</a></ng-template>
    @if (videoControls; as vc) {
      <x-child [label]="vc" data-hint="mute" [data-hint-value]="size()" />
    } @else if (model3dControls) {
      <ng-container *ngTemplateOutlet="menu; context: { $implicit: 'é' }" />
    }
    <p *ngIf="open; else closed">open</p>
    <i #note>note</i>
    <ng-template #closed><x-kid label="closed" /></ng-template>
    @for (row of rows; track row; let i = $index) {
      <b (click)="select(i)">{{ note.textContent }}</b>
    }
    @let total = rows.length;
  `,
})
export class Panel {
  /** Playback controls of the open video overlay. */
  get videoControls() { return 'video'; }
  /** Viewer controls of the open 3D overlay. */
  model3dControls = false;
  open = true;
  rows = [1, 2];
  readonly size = input(1);
  readonly changed = output<number>();
  readonly variant = computed(() => 'panel');
  @HostBinding('class.active') active = true;
  @HostListener('click', ['$event']) onClick(_e: unknown) {}
  onKey(_e: unknown) {}
  select(_i: number) {}
}
