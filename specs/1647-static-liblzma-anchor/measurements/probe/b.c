#include <lzma.h>
#include <stdio.h>
int main(void){uint8_t in[16]={0},out[256];size_t pos=0;
 lzma_ret r=lzma_easy_buffer_encode(6,LZMA_CHECK_CRC64,NULL,in,16,out,&pos,256);printf("%d %zu %s\n",r,pos,lzma_version_string());return 0;}
