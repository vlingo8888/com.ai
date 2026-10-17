export default async function PhotoModal({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  return <dialog open>Photo {id} in a modal</dialog>;
}
