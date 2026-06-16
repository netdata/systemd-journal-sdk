import struct

from .binary import align8
from .header import (
    FIELD_OBJECT_HEADER_SIZE, HEADER_SIZE, OBJECT_HEADER_SIZE, OBJECT_TYPE_DATA,
    OBJECT_TYPE_DATA_HASH_TABLE,
    OBJECT_TYPE_ENTRY, OBJECT_TYPE_ENTRY_ARRAY, OBJECT_TYPE_FIELD,
    OBJECT_TYPE_FIELD_HASH_TABLE, serialize_file_header,
    write_object_header,
)
from .seal import OBJECT_TYPE_TAG, TAG_LENGTH


class _WriterSealingMixin:
    def _append_tag(self):
        if self._seal is None:
            return
        self._seal.hmac_start()
        offset = self._append_offset
        size = OBJECT_HEADER_SIZE + 8 + 8 + TAG_LENGTH
        seqnum = self._header['n_tags'] + 1
        epoch = self._seal.get_epoch()
        self._ensure_arena_size(offset + align8(size))
        buf = bytearray(align8(size))
        write_object_header(buf, 0, OBJECT_TYPE_TAG, 0, size)
        struct.pack_into('<Q', buf, OBJECT_HEADER_SIZE, seqnum)
        struct.pack_into('<Q', buf, OBJECT_HEADER_SIZE + 8, epoch)
        self._seal.hmac_write(bytes(buf[:OBJECT_HEADER_SIZE + 16]))
        buf[OBJECT_HEADER_SIZE + 16:OBJECT_HEADER_SIZE + 16 + TAG_LENGTH] = self._seal.hmac_sum()
        self._write_at(offset, buf)
        self._object_added(offset, size)
        self._header['n_tags'] = seqnum
        self._seal.hmac_reset()

    def _append_first_tag(self):
        if self._seal is None:
            return
        self._hmac_put_header()
        self._hmac_put_hash_table_object(self._header['field_hash_table_offset'] - OBJECT_HEADER_SIZE)
        self._hmac_put_hash_table_object(self._header['data_hash_table_offset'] - OBJECT_HEADER_SIZE)
        self._append_tag()

    def _maybe_append_tag(self, realtime):
        if self._seal is None:
            return
        need = self._seal.need_evolve(realtime)
        if not need:
            return
        self._append_tag()
        while True:
            goal = self._seal.get_goal_epoch(realtime)
            epoch = self._seal.get_epoch()
            if epoch >= goal:
                break
            self._seal.evolve_state()
            if self._seal.get_epoch() < goal:
                self._append_tag()

    def _hmac_put_header(self):
        if self._seal is None:
            return
        self._seal.hmac_start()
        header_buf = bytearray(HEADER_SIZE)
        serialize_file_header(header_buf, self._header)
        self._seal.hmac_write(bytes(header_buf[0:16]))
        self._seal.hmac_write(bytes(header_buf[24:56]))
        self._seal.hmac_write(bytes(header_buf[72:96]))
        self._seal.hmac_write(bytes(header_buf[104:136]))

    def _hmac_put_hash_table_object(self, object_start):
        if self._seal is None:
            return
        self._seal.hmac_start()
        buf = self._read_at(object_start, OBJECT_HEADER_SIZE)
        self._seal.hmac_write(buf)

    def _hmac_put_object(self, object_start, typ):
        if self._seal is None:
            return
        self._seal.hmac_start()
        buf = self._read_at(object_start, OBJECT_HEADER_SIZE)
        self._seal.hmac_write(buf)
        obj_size = struct.unpack_from('<Q', buf, 8)[0]
        if typ == OBJECT_TYPE_DATA:
            hash_buf = self._read_at(object_start + 16, 8)
            self._seal.hmac_write(hash_buf)
            payload_offset = self._data_payload_offset()
            payload_size = obj_size - payload_offset
            if payload_size > 0:
                payload = self._read_at(object_start + payload_offset, payload_size)
                self._seal.hmac_write(payload)
        elif typ == OBJECT_TYPE_FIELD:
            hash_buf = self._read_at(object_start + 16, 8)
            self._seal.hmac_write(hash_buf)
            payload_size = obj_size - FIELD_OBJECT_HEADER_SIZE
            if payload_size > 0:
                payload = self._read_at(object_start + FIELD_OBJECT_HEADER_SIZE, payload_size)
                self._seal.hmac_write(payload)
        elif typ == OBJECT_TYPE_ENTRY:
            rest_size = obj_size - OBJECT_HEADER_SIZE
            if rest_size > 0:
                rest = self._read_at(object_start + OBJECT_HEADER_SIZE, rest_size)
                self._seal.hmac_write(rest)
        elif typ in (OBJECT_TYPE_DATA_HASH_TABLE, OBJECT_TYPE_FIELD_HASH_TABLE, OBJECT_TYPE_ENTRY_ARRAY):
            pass
        elif typ == OBJECT_TYPE_TAG:
            meta = self._read_at(object_start + OBJECT_HEADER_SIZE, 16)
            self._seal.hmac_write(meta)
