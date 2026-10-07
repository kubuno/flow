interface FlowLogoProps {
  size?:      number
  className?: string
  title?:     string
}

/** Flow logo (designer artwork, raster). Served by the host from
 *  `/flow-logo.png`; rendered as a square image so it weighs the same as its
 *  neighbours in the waffle menu. */
export function FlowLogo({ size = 24, className, title = 'Flow' }: FlowLogoProps) {
  return (
    <img
      src="/flow-logo.png"
      width={size}
      height={size}
      alt={title}
      className={className}
      style={{ display: 'block', objectFit: 'contain' }}
    />
  )
}

export default FlowLogo
