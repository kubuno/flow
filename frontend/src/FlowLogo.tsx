interface FlowLogoProps {
  size?:      number
  className?: string
  title?:     string
}

/** Logo Flow : carré arrondi bleu + trois nœuds blancs reliés par des flèches. */
export function FlowLogo({ size = 24, className, title = 'Flow' }: FlowLogoProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 1180 1180"
      role="img"
      aria-label={title}
      className={className}
      style={{ fillRule: 'evenodd', clipRule: 'evenodd', strokeLinecap: 'round' }}
    >
      <title>{title}</title>
      <g>
        <path d="M1179.167,294.792l0,589.583c0,162.7 -132.092,294.792 -294.792,294.792l-589.583,0c-162.7,0 -294.792,-132.092 -294.792,-294.792l0,-589.583c0,-162.7 132.092,-294.792 294.792,-294.792l589.583,0c162.7,0 294.792,132.092 294.792,294.792Z" style={{ fill: '#ebb925' }} />
        <path d="M382.758,518.651l0,141.865c0,32.195 -26.138,58.333 -58.333,58.333l-64.305,0c-32.195,0 -58.333,-26.138 -58.333,-58.333l0,-141.865c0,-32.195 26.138,-58.333 58.333,-58.333l64.305,0c32.195,0 58.333,26.138 58.333,58.333Z" style={{ fill: '#fff' }} />
        <path d="M680.069,518.651l0,141.865c0,32.195 -26.138,58.333 -58.333,58.333l-64.305,0c-32.195,0 -58.333,-26.138 -58.333,-58.333l0,-141.865c0,-32.195 26.138,-58.333 58.333,-58.333l64.305,0c32.195,0 58.333,26.138 58.333,58.333Z" style={{ fill: '#fff' }} />
        <path d="M977.381,518.651l0,141.865c0,32.195 -26.138,58.333 -58.333,58.333l-64.305,0c-32.195,0 -58.333,-26.138 -58.333,-58.333l0,-141.865c0,-32.195 26.138,-58.333 58.333,-58.333l64.305,0c32.195,0 58.333,26.138 58.333,58.333Z" style={{ fill: '#fff' }} />
        <path d="M382.758,589.583l71.096,0" style={{ fill: 'none', fillRule: 'nonzero', stroke: '#fff', strokeWidth: '28.87px' }} />
        <path d="M501.008,589.583l-78.535,-68.792l0,137.583l78.535,-68.792Z" style={{ fill: '#fff', fillRule: 'nonzero' }} />
        <path d="M680.069,589.583l71.096,0" style={{ fill: 'none', fillRule: 'nonzero', stroke: '#fff', strokeWidth: '28.87px' }} />
        <path d="M798.319,589.583l-78.535,-68.792l0,137.583l78.535,-68.792Z" style={{ fill: '#fff', fillRule: 'nonzero' }} />
      </g>
    </svg>
  )
}

export default FlowLogo
